#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Install a separately pinned research worker; no order or website authority."""

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
import time

READER_SHA256 = "12efd7e4fd958138840dc0056f84fe6ba5690cb7de01ce02065131e6cb2112d0"
LIBRARY = "/usr/local/lib/radar-research"
FILES = {
    "run-alpaca-research.py": (LIBRARY + "/run-alpaca-research.py", 0o755),
    "radar-paper-research.service": ("/etc/systemd/system/radar-paper-research.service", 0o644),
    "radar-paper-research.timer": ("/etc/systemd/system/radar-paper-research.timer", 0o644),
    "alpaca-research.json.example": ("/etc/radar/alpaca-research.json", 0o644),
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
        raise ValueError("Research bundle hash mismatch")
    result = {}
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
        for entry in archive:
            if entry.name not in FILES or entry.name in result or not entry.isfile() or entry.size > 65536:
                raise ValueError("Unexpected research bundle member")
            result[entry.name] = archive.extractfile(entry).read()
    if set(result) != set(FILES):
        raise ValueError("Incomplete research bundle")
    return result


def install(path, raw, mode):
    descriptor, temporary = tempfile.mkstemp(prefix=".research-", dir=path.parent)
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


def main():
    if len(sys.argv) != 2 or os.geteuid() != 0:
        raise ValueError("Run with sudo and the verified research bundle SHA256")
    contents = bundle(read("/tmp/radar-research-bundle.tar", 524288), sys.argv[1])
    reader = read("/tmp/radar-research.new", 32 * 1024 * 1024)
    if hashlib.sha256(reader).hexdigest() != READER_SHA256:
        raise ValueError("Expected verified research release")
    credentials = Path("/etc/radar/alpaca-paper.env")
    protected(credentials.parent)
    info = credentials.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or stat.S_IMODE(info.st_mode) != 0o600:
        raise ValueError("Expected private paper credentials")
    clean = {"PATH": "/usr/bin:/bin"}
    subprocess.run(["/usr/bin/systemctl", "is-active", "--quiet", "radar-codex.socket"], check=True, env=clean)
    library = Path(LIBRARY)
    protected(library.parent)
    for target in [library / "radar", *(Path(target) for target, _ in FILES.values())]:
        if target.exists() or target.is_symlink():
            raise ValueError("Research already provisioned; explicit upgrade required")
        if target.parent != library:
            protected(target.parent)
    # Freeze twelve hypothetical research slots, not an account capital mandate.
    plan = json.loads(contents["alpaca-research.json.example"])
    if plan.get("version") != "alpaca-research-v1" or plan.get("start_unix_ms") != 0:
        raise ValueError("Expected reviewed research template")
    plan["start_unix_ms"] = time.time_ns() // 1000000 + 120000
    contents["alpaca-research.json.example"] = json.dumps(plan, sort_keys=True).encode()
    library.mkdir(mode=0o755, exist_ok=True)
    protected(library)
    install(library / "radar", reader, 0o755)
    for name, (target, mode) in FILES.items():
        install(Path(target), contents[name], mode)
    subprocess.run(["/usr/bin/systemctl", "daemon-reload"], check=True, env=clean)
    # Initial tick freezes future slots without consuming a model call.
    subprocess.run(["/usr/bin/systemctl", "start", "radar-paper-research.service"], check=True, env=clean)
    subprocess.run(["/usr/bin/systemctl", "enable", "--now", "radar-paper-research.timer"], check=True, env=clean)
    print("Research campaign enabled: 12 scheduled attempts. No trading authority enabled.")


if __name__ == "__main__":
    try:
        main()
    except Exception:
        sys.exit("Research setup failed; existing files retained, trading authority unchanged")
