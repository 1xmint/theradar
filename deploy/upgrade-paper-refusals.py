#!/usr/bin/python3 -I
# SPDX-License-Identifier: Apache-2.0
"""Upgrade a known paused private paper install; never bootstrap or grant authority."""

import hashlib
import io
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tarfile
import tempfile

ACCOUNT = "374d3d8e-63ef-41d1-a1a3-b7921776fbc4"
LIBRARY = "/usr/local/lib/radar-paper-control"
STATE = "/var/lib/private/radar-paper-execution"
OLD = {"radar":"0c421ea6475f400cda75126214725f718ed37cede13fe68d72c5f663016494f9",
    "paper-control.py":"d0bd5acc9b0fd4f62eab980f691a7a3aa92a7990b88cb656a747b9fb404621b3",
    "paper-supervisor.py":"db74d454bcaf721c21dcd94daa13f05d86d411b73a25ca4981432ed538bfdc76"}
READER_HASH = "47b0448dd16b89818bcf65551745814161cc7ac082e1ff9618c143b409c30a3f"


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def read(path, limit=32 * 1024 * 1024):
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError("Regular upgrade input required")
        raw = stream.read(limit + 1)
    if len(raw) > limit:
        raise ValueError("Upgrade input exceeds bound")
    return raw


def protected(path):
    for directory in (path, *path.parents):
        info = directory.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
            raise ValueError("Protected upgrade directory required")


def publish(path, raw, mode, replace=False):
    descriptor, temporary = tempfile.mkstemp(prefix=".refusal-upgrade-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(raw)
            os.fchmod(stream.fileno(), mode)
            stream.flush()
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


def payload(raw, expected):
    if not re.fullmatch(r"[a-f0-9]{64}", expected) or digest(raw) != expected:
        raise ValueError("Upgrade archive hash differs")
    result = {}
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
        for member in archive:
            if member.name not in ("paper-control.py", "paper-supervisor.py") or member.name in result or not member.isfile() or member.size > 65536:
                raise ValueError("Unexpected upgrade archive member")
            result[member.name] = archive.extractfile(member).read()
    if set(result) != {"paper-control.py", "paper-supervisor.py"}:
        raise ValueError("Incomplete upgrade archive")
    if ('READER_HASH = "' + READER_HASH + '"').encode() not in result["paper-control.py"]:
        raise ValueError("Controller must pin upgraded reader")
    return result


class Upgrade:
    def __init__(self, root=Path("/"), run=None):
        self.root, self.run = root, run or self.command

    def path(self, name):
        return self.root / name.lstrip("/")

    @staticmethod
    def command(argv):
        return subprocess.run(argv, check=True, capture_output=True, text=True, env={"PATH":"/usr/bin:/bin"})

    def inactive(self, service):
        if self.run(["/usr/bin/systemctl", "show", service, "--property=ActiveState", "--value"]).stdout.strip() != "inactive":
            raise ValueError("Active paper tick requires inspection")

    def unconfigured(self):
        state = self.path(STATE)
        if (state / "control.json").exists() or (state / "control.json").is_symlink() or list(state.glob("effective-policy-*.json")):
            raise ValueError("Upgrade requires unconfigured handover")

    def apply(self, raw, expected, reader):
        if os.geteuid() != 0:
            raise ValueError("Root upgrade required")
        contents = payload(raw, expected)
        if digest(reader) != READER_HASH:
            raise ValueError("Upgraded reader hash differs")
        library = self.path(LIBRARY)
        protected(library)
        before = {}
        for name, expected_old in OLD.items():
            target = library / name
            info = target.lstat()
            before[name] = read(target)
            if info.st_uid != 0 or info.st_mode & 0o022 or digest(before[name]) != expected_old:
                raise ValueError("Installed paper payload differs")
        backup = library / "before-refusal-upgrade-70aabc9"
        if backup.exists() or backup.is_symlink():
            raise ValueError("Prior upgrade evidence exists; do not retry")
        self.unconfigured()
        state = self.path(STATE)
        markers = {name:read(state / name, 65536) for name in (".lock", "genesis-complete.json")}
        if json.loads(markers["genesis-complete.json"]) != {"genesis":"initialized", "account_id":ACCOUNT, "new_order_authority":False}:
            raise ValueError("Existing pinned broker genesis required")
        journal = read(state / "orders.jsonl")
        if not journal:
            raise ValueError("Existing broker history required")
        self.run(["/usr/bin/systemctl", "stop", "radar-paper-supervisor.timer"])
        self.inactive("radar-paper-supervisor.service")
        self.unconfigured()
        self.run(["/usr/bin/systemctl", "stop", "radar-paper-control.socket", "radar-paper-control.service"])
        self.inactive("radar-paper-control.service")
        self.unconfigured()
        backup.mkdir(mode=0o755)
        for name, value in before.items():
            if read(library / name) != value:
                raise ValueError("Installed payload changed during quiescence")
            publish(backup / name, value, 0o755 if name == "radar" else 0o644)
        for name, value in dict(contents, radar=reader).items():
            publish(library / name, value, 0o755 if name == "radar" else 0o644, replace=True)
        self.run(["/usr/bin/systemctl", "start", "radar-paper-control.socket"])
        self.run(["/usr/bin/systemctl", "start", "radar-paper-supervisor.service"])
        self.unconfigured()
        if not read(state / "orders.jsonl").startswith(journal) or any(read(state / name, 65536) != value for name, value in markers.items()):
            raise ValueError("History preservation check failed")
        self.run(["/usr/bin/systemctl", "start", "radar-paper-supervisor.timer"])
        return dict(upgraded=True, original_prefix_preserved=True, handover_enabled=False, new_order_authority=False)


def main():
    if len(sys.argv) != 2:
        raise ValueError("Expected reviewed archive hash")
    result = Upgrade().apply(read(Path("/tmp/radar-paper-refusal-bundle.tar"), 256 * 1024), sys.argv[1],
        read(Path("/tmp/radar-paper-refusal-reader.new")))
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    try: main()
    except Exception: sys.exit("Paper upgrade unresolved; preserve files and inspect, do not retry")
