#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Relay three fixed vendor-CLI operations over a local, systemd-owned socket.

This process never reads auth.json. The vendor CLI runs as radar-agent and owns
credentials. The client can run under Radar's NoNewPrivileges restriction.
"""

import base64
import json
import os
import pwd
import selectors
import signal
import socket
import struct
import subprocess
import sys
import threading
import time

SOCKET = "/run/radar-codex.sock"
HOME = "/var/lib/radar-agent"
MAX_INPUT = 128 * 1024
MAX_REQUEST = 1024 * 1024
MAX_OUTPUT = 4 * 1024 * 1024
COMMANDS = {
    ("login", "--device-auth"): (["login", "--device-auth"], 900),
    ("login", "status"): (["login", "status"], 15),
    ("exec", "-"): (["exec", "--cd", HOME + "/work", "--skip-git-repo-check",
                      "--sandbox", "read-only", "--ephemeral", "--color", "never", "-"], 180),
}


def send(connection, packet):
    connection.sendall(json.dumps(packet).encode() + b"\n")


def stop(child):
    if child is not None:
        try:
            os.killpg(child.pid, signal.SIGTERM)
            child.wait(timeout=3)
        except ProcessLookupError:
            pass
        except subprocess.TimeoutExpired:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait()


def serve_one(connection, expected_uid):
    child = None
    try:
        _, uid, _ = struct.unpack("3i", connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
        if uid != expected_uid:
            raise ValueError("caller refused")
        connection.settimeout(5)
        reader = connection.makefile("rb")
        line = reader.readline(MAX_REQUEST + 1)
        if len(line) > MAX_REQUEST or not line.endswith(b"\n"):
            raise ValueError("request too large or incomplete")
        request = json.loads(line)
        if (not isinstance(request, dict) or set(request) != {"command", "input"}
                or not isinstance(request["command"], list)
                or not all(isinstance(arg, str) for arg in request["command"])
                or not isinstance(request["input"], str)):
            raise ValueError("request refused")
        command = tuple(request["command"])
        if command not in COMMANDS:
            raise ValueError("command refused")
        prompt = request["input"].encode("utf-8")
        if len(prompt) > MAX_INPUT or (command != ("exec", "-") and prompt):
            raise ValueError("input refused")
        argv, duration = COMMANDS[command]
        child = subprocess.Popen(
            ["/usr/bin/codex", *argv], cwd=HOME + "/work",
            env={"PATH": "/usr/bin:/bin", "HOME": HOME, "CODEX_HOME": HOME + "/.codex",
                 "LANG": "C.UTF-8"},
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            start_new_session=True,
        )

        def feed():
            try:
                child.stdin.write(prompt)
                child.stdin.close()
            except (ValueError, OSError):
                pass

        threading.Thread(target=feed, daemon=True).start()
        connection.settimeout(5)
        with selectors.DefaultSelector() as selector:
            selector.register(connection, selectors.EVENT_READ, "client")
            selector.register(child.stdout, selectors.EVENT_READ, "stdout")
            selector.register(child.stderr, selectors.EVENT_READ, "stderr")
            deadline = time.monotonic() + duration
            output = 0
            streams = 2
            while streams or child.poll() is None:
                if time.monotonic() >= deadline:
                    raise ValueError("vendor CLI timed out")
                for key, _ in selector.select(0.25):
                    if key.data == "client":
                        # The request is complete. EOF means Radar killed its client.
                        if not connection.recv(1):
                            return
                        raise ValueError("unexpected request data")
                    data = os.read(key.fileobj.fileno(), 16384)
                    if not data:
                        selector.unregister(key.fileobj)
                        streams -= 1
                        continue
                    output += len(data)
                    if output > MAX_OUTPUT:
                        raise ValueError("vendor CLI output exceeded limit")
                    send(connection, {"stream": key.data, "data": base64.b64encode(data).decode()})
            send(connection, {"exit": child.wait()})
    except (ValueError, KeyError, TypeError, OSError):
        # No request, prompt, credential or vendor error body goes to the journal.
        try:
            send(connection, {"error": "Radar Codex request could not complete"})
        except OSError:
            pass
    finally:
        stop(child)
        connection.close()


def serve():
    if os.environ.get("LISTEN_PID") != str(os.getpid()) or os.environ.get("LISTEN_FDS") != "1":
        raise SystemExit("Start through radar-codex.socket")
    listener = socket.socket(fileno=3)
    guardian_uid = pwd.getpwnam("guardian").pw_uid
    # One CLI writer, including refresh during inference and device login.
    slots = threading.BoundedSemaphore(1)

    def handle(connection):
        try:
            serve_one(connection, guardian_uid)
        finally:
            slots.release()

    while True:
        connection, _ = listener.accept()
        if not slots.acquire(blocking=False):
            connection.settimeout(1)
            try:
                send(connection, {"error": "Radar Codex service is busy"})
            except OSError:
                pass
            connection.close()
            continue
        threading.Thread(target=handle, args=(connection,), daemon=True).start()


def client():
    command = tuple(sys.argv[1:])
    if command not in COMMANDS:
        raise SystemExit("Unsupported Radar Codex invocation")
    prompt = sys.stdin.buffer.read(MAX_INPUT + 1) if command == ("exec", "-") else b""
    if len(prompt) > MAX_INPUT:
        raise SystemExit("Radar Codex input exceeded limit")
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
        connection.connect(SOCKET)
        send(connection, {"command": list(command), "input": prompt.decode("utf-8")})
        reader = connection.makefile("rb")
        for line in reader:
            packet = json.loads(line)
            if "exit" in packet:
                return packet["exit"] if packet["exit"] >= 0 else 1
            if "error" in packet:
                print(packet["error"], file=sys.stderr)
                return 1
            stream = {"stdout": sys.stdout.buffer, "stderr": sys.stderr.buffer}[packet["stream"]]
            stream.write(base64.b64decode(packet["data"], validate=True))
            stream.flush()
    return 1


if __name__ == "__main__":
    if sys.argv[1:] == ["--serve"]:
        serve()
    else:
        try:
            sys.exit(client())
        except (OSError, ValueError, KeyError, UnicodeError):
            raise SystemExit("Radar Codex service is unavailable")
