# SPDX-License-Identifier: Apache-2.0
"""Linux socket/process checks with a fake CLI; no vendor login or inference."""

import base64
import importlib.util
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import threading
import types
import unittest


@unittest.skipUnless(sys.platform == "linux", "SO_PEERCRED is a Linux boundary")
class BridgeTests(unittest.TestCase):
    def setUp(self):
        spec = importlib.util.spec_from_file_location("bridge", Path(__file__).with_name("radar-codex-bridge.py"))
        self.bridge = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.bridge)
        self.spawned = []
        self.invocations = []
        self.stub = "import sys; data=sys.stdin.buffer.read(); sys.stdout.buffer.write(data); print('progress',file=sys.stderr)"

        def spawn(argv, **kwargs):
            self.invocations.append((argv, kwargs))
            child = subprocess.Popen([sys.executable, "-c", self.stub], **{k: v for k, v in kwargs.items() if k != "cwd"})
            self.spawned.append(child)
            return child

        self.bridge.subprocess = types.SimpleNamespace(
            Popen=spawn, PIPE=subprocess.PIPE, TimeoutExpired=subprocess.TimeoutExpired,
        )

    def request(self, request, uid=None):
        local, remote = socket.socketpair()
        worker = threading.Thread(target=self.bridge.serve_one, args=(remote, os.getuid() if uid is None else uid))
        packets = []
        with local:
            local.settimeout(5)
            local.sendall(json.dumps(request).encode() + b"\n")
            worker.start()
            with local.makefile("rb") as reader:
                for line in reader:
                    packet = json.loads(line)
                    packets.append(packet)
                    if "exit" in packet or "error" in packet:
                        break
        worker.join(5)
        self.assertFalse(worker.is_alive(), "request worker did not terminate")
        return packets

    def test_fixed_exec_preserves_streams_stdin_and_clean_environment(self):
        prompt = 'Untrusted metadata: $(touch /tmp/never-run); "token"\n'
        packets = self.request({"command": ["exec", "-"], "input": prompt})
        stdout = b"".join(base64.b64decode(p["data"]) for p in packets if p.get("stream") == "stdout")
        stderr = b"".join(base64.b64decode(p["data"]) for p in packets if p.get("stream") == "stderr")
        self.assertEqual(stdout, prompt.encode())
        self.assertEqual(stderr, b"progress\n")
        self.assertEqual(packets[-1], {"exit": 0})
        argv, options = self.invocations[0]
        self.assertEqual(argv, ["/usr/bin/codex", *self.bridge.COMMANDS[("exec", "-")][0]])
        self.assertNotIn(prompt, argv)
        self.assertEqual(set(options["env"]), {"PATH", "HOME", "CODEX_HOME", "LANG"})

    def test_extra_arguments_and_malformed_input_never_spawn(self):
        for request in (
            {"command": ["exec", "-", "--dangerously-bypass-approvals-and-sandbox"], "input": ""},
            {"command": ["login", "--device-auth"], "input": "unexpected"},
            {"command": ["exec", "-"], "input": 7},
            {"command": ["exec", "-"], "input": "x" * (self.bridge.MAX_INPUT + 1)},
        ):
            with self.subTest(request_type=type(request["input"]).__name__):
                self.assertIn("error", self.request(request)[-1])
        self.assertEqual(self.spawned, [])

    def test_a_different_peer_uid_never_spawns(self):
        packets = self.request({"command": ["login", "status"], "input": ""}, uid=os.getuid() + 1)
        self.assertIn("error", packets[-1])
        self.assertEqual(self.spawned, [])

    def test_disconnecting_kills_the_cli_process_group(self):
        self.stub = "import time; print('ready',flush=True); time.sleep(60)"
        local, remote = socket.socketpair()
        worker = threading.Thread(target=self.bridge.serve_one, args=(remote, os.getuid()))
        with local:
            local.settimeout(5)
            local.sendall(b'{"command":["login","--device-auth"],"input":""}\n')
            worker.start()
            with local.makefile("rb") as reader:
                self.assertEqual(json.loads(reader.readline())["stream"], "stdout")
        worker.join(5)
        self.assertFalse(worker.is_alive())
        self.assertIsNotNone(self.spawned[0].poll(), "CLI survived client disconnection")


if __name__ == "__main__":
    unittest.main()
