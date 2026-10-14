#!/usr/bin/python3 -I
# SPDX-License-Identifier: Apache-2.0
"""One reviewed private paper install. Preserve journal, secrets and web hardening."""

import hashlib
import io
import json
import os
from pathlib import Path
import pwd
import grp
import re
import stat
import subprocess
import sys
import tarfile
import tempfile

ACCOUNT = "374d3d8e-63ef-41d1-a1a3-b7921776fbc4"
READER_HASH = "0c421ea6475f400cda75126214725f718ed37cede13fe68d72c5f663016494f9"
BRIDGE_HASH = "4a6eba67dece415085dd71e7eab248eb22d8b56d21fe423b6a606bb1ec4cfcd8"
CLIENT_HASH = "37b37f06839bbb688df9813634a9814102a2f87e72a398046c162cd9284de31a"
LIBRARY = "/usr/local/lib/radar-paper-control"
BRIDGE = "/usr/local/lib/radar/radar-codex-bridge.py"
STATE = "/var/lib/private/radar-paper-execution"
ADMIN = "/usr/local/sbin/radar-paper-autonomy-admin"
RULE = "/etc/sudoers.d/radar-paper-autonomy-admin"
FILES = {
    "paper-control.py":(LIBRARY + "/paper-control.py", 0o644),
    "paper-supervisor.py":(LIBRARY + "/paper-supervisor.py", 0o644),
    "provision-paper-supervisor.py":(LIBRARY + "/provision-paper-supervisor.py", 0o644),
    "radar-paper-autonomy-admin.py":(ADMIN, 0o755),
    "radar-paper-control.service":("/etc/systemd/system/radar-paper-control.service", 0o644),
    "radar-paper-control.socket":("/etc/systemd/system/radar-paper-control.socket", 0o644),
    "radar-paper-supervisor.service":("/etc/systemd/system/radar-paper-supervisor.service", 0o644),
    "radar-paper-supervisor.timer":("/etc/systemd/system/radar-paper-supervisor.timer", 0o644),
    "radar-paper-web-ipc.conf":("/etc/systemd/system/radar-serve.service.d/paper-ipc.conf", 0o644),
    "radar-paper-codex-ipc.conf":("/etc/systemd/system/radar-codex.socket.d/paper-ipc.conf", 0o644),
    "radar-codex-bridge.py":(BRIDGE, 0o644),
}


def read(path, limit=65536):
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError("Expected regular installation input")
        raw = stream.read(limit + 1)
    if len(raw) > limit:
        raise ValueError("Installation input exceeds bound")
    return raw


def protected(directory):
    for path in (directory, *directory.parents):
        info = path.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
            raise ValueError("Unprotected installation directory")


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def private_environment(path):
    protected(path.parent)
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or stat.S_IMODE(info.st_mode) != 0o600:
        raise ValueError("Expected root-only paper environment")


def pinned(path, expected):
    protected(path.parent)
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022 or digest(read(path)) != expected:
        raise ValueError("Existing subscription installation differs")


def bundle(raw, expected):
    if not re.fullmatch(r"[a-f0-9]{64}", expected) or digest(raw) != expected:
        raise ValueError("Private installation bundle hash mismatch")
    result = {}
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
        for entry in archive:
            if entry.name not in FILES or entry.name in result or not entry.isfile() or entry.size > 65536:
                raise ValueError("Unexpected private bundle member")
            result[entry.name] = archive.extractfile(entry).read()
    if set(result) != set(FILES):
        raise ValueError("Incomplete private bundle")
    return result


def publish(path, raw, mode, replace=False):
    descriptor, temporary = tempfile.mkstemp(prefix=".paper-autonomy-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(raw)
            stream.flush()
            os.fchmod(stream.fileno(), mode)
            os.fsync(stream.fileno())
        if replace:
            os.replace(temporary, path)
        else:
            os.link(temporary, path)
        descriptor = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try: os.fsync(descriptor)
        finally: os.close(descriptor)
    finally:
        if os.path.exists(temporary): os.unlink(temporary)


def call(argv):
    return subprocess.run(argv, check=True, capture_output=True, text=True, timeout=100, env={"PATH":"/usr/bin:/bin"})


class Setup:
    def __init__(self, root=Path("/"), run=call):
        self.root, self.run = root, run
        self.stage = "preflight"

    def path(self, absolute):
        return self.root / absolute.lstrip("/")

    def absent(self, path):
        if path.exists() or path.is_symlink():
            raise ValueError("Prior installation retained; explicit recovery required")

    def command(self, *args):
        return self.run(list(args))

    def install(self, raw, expected, reader):
        if os.geteuid() != 0:
            raise ValueError("One-time private installation requires root")
        contents = bundle(raw, expected)
        if digest(reader) != READER_HASH:
            raise ValueError("Expected verified paper stress reader")
        # Never read broker secrets; only verify their ownership and mode.
        credentials = self.path("/etc/radar/alpaca-paper.env")
        private_environment(credentials)
        for user in ("guardian", "radar-agent"):
            if pwd.getpwnam(user).pw_uid == 0:
                raise ValueError("Separate non-root principals required")
        if pwd.getpwnam("guardian").pw_uid == pwd.getpwnam("radar-agent").pw_uid:
            raise ValueError("Separate non-root principals required")
        try: pwd.getpwnam("radar-paper-supervisor")
        except KeyError: pass
        else: raise ValueError("Supervisor principal exists; explicit recovery required")
        for name in ("radar-paper-ipc", "radar-paper-supervisor"):
            try: grp.getgrnam(name)
            except KeyError: pass
            else: raise ValueError("Paper group exists; explicit recovery required")
        bridge = self.path(BRIDGE)
        for path, expected_hash in ((bridge, BRIDGE_HASH), (self.path("/usr/local/bin/radar-codex"), CLIENT_HASH)):
            pinned(path, expected_hash)
        library = self.path(LIBRARY)
        fresh = [library, self.path(RULE), bridge.with_name("radar-codex-bridge.py.before-paper-autonomy"),
            self.path("/etc/radar/paper-autonomy-origin.json"),
            *(self.path("/etc/radar/" + name + ".json") for name in ("alpaca-paper-control", "alpaca-paper-model", "alpaca-paper-supervisor")),
            self.path("/var/lib/radar-paper-supervisor"), self.path("/var/lib/private/radar-paper-supervisor")]
        fresh += [self.path(target) for target, _ in FILES.values() if target != BRIDGE]
        for path in fresh: self.absent(path)
        for directory in (library.parent, self.path(ADMIN).parent, self.path("/etc/systemd/system"), self.path("/etc/sudoers.d"), self.path("/var/lib/private")):
            protected(directory)
        for name in ("radar-serve.service.d", "radar-codex.socket.d"):
            directory = self.path("/etc/systemd/system/" + name)
            if directory.exists() or directory.is_symlink(): protected(directory)
        state = self.path(STATE)
        if not state.is_dir() or state.is_symlink():
            raise ValueError("Existing private broker state required")
        journal = read(state / "orders.jsonl", 32 * 1024 * 1024)
        markers = {name:read(state / name) for name in (".lock", "genesis-complete.json")}
        if not journal or json.loads(markers["genesis-complete.json"]) != {"genesis":"initialized", "account_id":ACCOUNT, "new_order_authority":False}:
            raise ValueError("Existing pinned broker genesis required")
        if (state / "control.json").exists() or list(state.glob("effective-policy-*.json")):
            raise ValueError("Existing owner policy requires explicit upgrade")
        rule = ("# Fixed paper timer administration; no policy or root shell.\n" +
            "guardian ALL=(root) NOPASSWD: " + ", ".join(ADMIN + " " + action for action in ("inspect", "start", "stop")) + "\n").encode()
        descriptor, temporary = tempfile.mkstemp(prefix=".paper-rule-check-", dir=self.path("/etc/sudoers.d"))
        try:
            with os.fdopen(descriptor, "wb") as stream: stream.write(rule)
            self.command("/usr/sbin/visudo", "-cf", temporary)
        finally: os.unlink(temporary)
        # Retire completed legacy campaigns and refuse a known active model call.
        self.stage = "retire_completed_research"
        for name in ("radar-paper-research", "radar-context-research"):
            self.command("/usr/bin/systemctl", "disable", "--now", name + ".timer")
            if self.command("/usr/bin/systemctl", "show", name + ".service", "--property=ActiveState", "--value").stdout.strip() != "inactive":
                raise ValueError("Legacy research still active; inspect rather than terminate")
        if self.command("/usr/bin/systemctl", "show", "radar-codex.service", "--property=TasksCurrent", "--value").stdout.strip() not in ("0", "1"):
            raise ValueError("Subscription call active; installation deferred")
        self.stage = "subscription_quiescence"
        self.command("/usr/bin/systemctl", "stop", "radar-codex.socket")
        self.command("/usr/bin/systemctl", "stop", "radar-codex.service")
        self.stage = "protected_installation"
        publish(self.path("/etc/radar/paper-autonomy-origin.json"), json.dumps(dict(account_id=ACCOUNT,
            original_journal_bytes=len(journal), original_journal_sha256=digest(journal), new_order_authority=False)).encode(), 0o644)
        self.command("/usr/sbin/groupadd", "--system", "radar-paper-ipc")
        self.command("/usr/sbin/useradd", "--system", "--user-group", "--no-create-home", "--home-dir", "/nonexistent",
            "--shell", "/usr/sbin/nologin", "radar-paper-supervisor")
        self.command("/usr/sbin/usermod", "--append", "--groups", "radar-paper-ipc", "guardian")
        library.mkdir(mode=0o755)
        for name in ("radar-serve.service.d", "radar-codex.socket.d"):
            self.path("/etc/systemd/system/" + name).mkdir(mode=0o755, exist_ok=True)
        publish(library / "radar", reader, 0o755)
        scenario = dict(version="alpaca-paper-stress-v1", account_id=ACCOUNT, max_quantity_nanoshares=1_000_000_000,
            fee_per_side_microusd=10000, slippage_bps_per_side=10)
        model = json.dumps(scenario, sort_keys=True).encode()
        settings = {"alpaca-paper-model":model,
            "alpaca-paper-control":json.dumps(dict(version="alpaca-paper-control-v1", account_id=ACCOUNT,
                operator_email="jfair1028@gmail.com", model_sha256=digest(model))).encode(),
            "alpaca-paper-supervisor":json.dumps(dict(version="alpaca-paper-supervisor-v1", account_id=ACCOUNT)).encode()}
        for name, value in settings.items(): publish(self.path("/etc/radar/" + name + ".json"), value, 0o644)
        for name, (target, mode) in FILES.items():
            if target != BRIDGE: publish(self.path(target), contents[name], mode)
        if digest(read(bridge)) != BRIDGE_HASH: raise ValueError("Subscription bridge changed during install")
        publish(bridge.with_name("radar-codex-bridge.py.before-paper-autonomy"), read(bridge), 0o644)
        publish(bridge, contents["radar-codex-bridge.py"], 0o644, replace=True)
        publish(self.path(RULE), rule, 0o440)
        self.command("/usr/bin/systemctl", "daemon-reload")
        self.stage = "supervisor_origin"
        provision = ["/usr/bin/systemd-run", "--wait", "--pipe", "--collect", "--unit=radar-paper-autonomy-provision"]
        for value in ("User=radar-paper-supervisor", "Group=radar-paper-supervisor", "StateDirectory=radar-paper-supervisor",
                "StateDirectoryMode=0700", "NoNewPrivileges=yes", "ProtectSystem=strict", "ProtectHome=yes", "PrivateTmp=yes",
                "PrivateDevices=yes", "CapabilityBoundingSet=", "RestrictAddressFamilies=AF_UNIX", "UMask=0077", "RuntimeMaxSec=30"):
            provision += ["--property", value]
        self.run(provision + ["/usr/bin/python3", "-I", LIBRARY + "/provision-paper-supervisor.py"])
        self.stage = "initial_paused_tick"
        self.command("/usr/bin/systemctl", "enable", "--now", "radar-codex.socket", "radar-paper-control.socket")
        self.command("/usr/bin/systemctl", "start", "radar-paper-supervisor.service")
        # Paused observation may append a checkpoint, never rewrite old bytes.
        self.stage = "history_preservation"
        after = read(state / "orders.jsonl", 32 * 1024 * 1024)
        if not after.startswith(journal) or any(read(state / name) != value for name, value in markers.items()):
            raise ValueError("Original broker history changed; retain installation for inspection")
        if (state / "control.json").exists() or list(state.glob("effective-policy-*.json")):
            raise ValueError("Initial paper handover must remain unconfigured")
        self.command("/usr/bin/systemctl", "enable", "--now", "radar-paper-supervisor.timer")
        self.stage = "complete"
        return dict(private_paper_installed=True, original_prefix_preserved=True, handover_enabled=False,
            new_order_authority=False, website_restart_required=True)


if __name__ == "__main__":
    installation = Setup()
    try:
        if len(sys.argv) != 2: raise ValueError("One reviewed bundle hash required")
        print(json.dumps(installation.install(read(Path("/tmp/radar-paper-autonomy-bundle.tar"), 1024 * 1024), sys.argv[1],
            read(Path("/tmp/radar-paper-autonomy-reader.new"), 32 * 1024 * 1024)), sort_keys=True))
    except Exception:
        sys.exit("Private paper setup incomplete at " + installation.stage + "; inspect retained files and services, never delete or retry over history")
