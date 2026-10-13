#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Install verified paper observation/maintenance, never initialize over prior state."""

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
import uuid

READER_SHA256 = "3f21fc1ae3b54b2333a5b809eda916c296be0948d69dba1f9da618adec9733d2"
LIBRARY = "/usr/local/lib/radar-paper-execution"
STATES = ("/var/lib/radar-paper-execution", "/var/lib/private/radar-paper-execution")
FILES = {
    "maintain-alpaca-paper.py": (LIBRARY + "/maintain-alpaca-paper.py", 0o755),
    "initialize-alpaca-paper.py": (LIBRARY + "/initialize-alpaca-paper.py", 0o755),
    "radar-paper-maintenance.service": ("/etc/systemd/system/radar-paper-maintenance.service", 0o644),
    "radar-paper-maintenance.timer": ("/etc/systemd/system/radar-paper-maintenance.timer", 0o644),
    "radar-paper-initialize.service": ("/etc/systemd/system/radar-paper-initialize.service", 0o644),
    "alpaca-maintenance.json.example": ("/etc/radar/alpaca-maintenance.json", 0o644),
}


def read(path, limit):
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError("Expected regular setup input")
        raw = stream.read(limit + 1)
    if len(raw) > limit:
        raise ValueError("Setup input too large")
    return raw


def protected(path):
    for directory in [path, *path.parents]:
        info = directory.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
            raise ValueError("Unsafe installation directory")


def bundle(raw, expected):
    if not re.fullmatch(r"[0-9a-f]{64}", expected) or hashlib.sha256(raw).hexdigest() != expected:
        raise ValueError("Execution bundle hash mismatch")
    result = {}
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
        for entry in archive:
            if entry.name not in FILES or entry.name in result or not entry.isfile() or entry.size > 65536:
                raise ValueError("Unexpected execution bundle member")
            result[entry.name] = archive.extractfile(entry).read()
    if set(result) != set(FILES):
        raise ValueError("Incomplete execution bundle")
    return result


def install(path, raw, mode):
    descriptor, temporary = tempfile.mkstemp(prefix=".execution-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(raw)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary, mode)
        os.link(temporary, path)
        descriptor = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)
    finally:
        os.unlink(temporary)


def validate(plan):
    if (not isinstance(plan, dict) or set(plan) != {"version", "account_id", "max_pages", "mode"}
            or plan["version"] != "alpaca-maintenance-v1" or plan["mode"] != "observe"
            or not isinstance(plan["account_id"], str)
            or str(uuid.UUID(plan["account_id"])) != plan["account_id"]
            or type(plan["max_pages"]) is not int or not 1 <= plan["max_pages"] <= 10):
        raise ValueError("Expected reviewed observation-only paper configuration")


def main():
    if len(sys.argv) != 2 or os.geteuid() != 0:
        raise ValueError("Run with sudo and verified execution bundle SHA256")
    contents = bundle(read("/tmp/radar-paper-execution-bundle.tar", 524288), sys.argv[1])
    reader = read("/tmp/radar-paper-execution.new", 32 * 1024 * 1024)
    if hashlib.sha256(reader).hexdigest() != READER_SHA256:
        raise ValueError("Expected verified maintenance release")
    credentials = Path("/etc/radar/alpaca-paper.env")
    protected(credentials.parent)
    info = credentials.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or stat.S_IMODE(info.st_mode) != 0o600:
        raise ValueError("Expected private paper credentials")
    library = Path(LIBRARY)
    protected(library.parent)
    protected(Path("/var/lib"))
    if Path("/var/lib/private").exists() or Path("/var/lib/private").is_symlink():
        protected(Path("/var/lib/private"))
    for prior in (library, *(Path(path) for path in STATES)):
        if prior.exists() or prior.is_symlink():
            raise ValueError("Execution previously provisioned; explicit recovery required")
    for target, _ in FILES.values():
        path = Path(target)
        if path.exists() or path.is_symlink():
            raise ValueError("Execution target exists; explicit upgrade required")
        if path.parent != library:
            protected(path.parent)
    validate(json.loads(contents["alpaca-maintenance.json.example"]))
    library.mkdir(mode=0o755)
    protected(library)
    install(library / "radar", reader, 0o755)
    for name, (target, mode) in FILES.items():
        install(Path(target), contents[name], mode)
    clean = {"PATH": "/usr/bin:/bin"}
    subprocess.run(["/usr/bin/systemctl", "daemon-reload"], check=True, env=clean)
    subprocess.run(["/usr/bin/systemctl", "start", "radar-paper-initialize.service"], check=True, env=clean)
    subprocess.run(["/usr/bin/systemctl", "start", "radar-paper-maintenance.service"], check=True, env=clean)
    subprocess.run(["/usr/bin/systemctl", "enable", "--now", "radar-paper-maintenance.timer"], check=True, env=clean)
    print("Paper maintenance observation enabled. Initial history retained; new orders remain disabled.")


if __name__ == "__main__":
    try:
        main()
    except Exception:
        sys.exit("Execution setup failed; inspect retained files and history, do not delete or retry")
