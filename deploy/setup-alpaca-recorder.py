#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Install a verified read-only recorder bundle; no execution service or website changes."""

import hashlib
import io
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tarfile
import tempfile

READER_SHA256 = "1ac8285579e39d474aa1eaaccba2a0b480306d089a45eb6b43cf737a98fe8dc0"
FILES = {
    "record-alpaca-market.py": ("/usr/local/lib/radar-paper/record-alpaca-market.py", 0o755),
    "radar-paper-market.service": ("/etc/systemd/system/radar-paper-market.service", 0o644),
    "radar-paper-market.timer": ("/etc/systemd/system/radar-paper-market.timer", 0o644),
}


def bundle(raw, expected):
    if (not re.fullmatch(r"[0-9a-f]{64}", expected)
            or hashlib.sha256(raw).hexdigest() != expected):
        raise ValueError("Recorder bundle hash mismatch")
    result = {}
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
        for entry in archive:
            if entry.name not in FILES or entry.name in result or not entry.isfile() or entry.size > 65536:
                raise ValueError("Unexpected recorder bundle entry")
            result[entry.name] = archive.extractfile(entry).read()
    if set(result) != set(FILES):
        raise ValueError("Incomplete recorder bundle")
    return result


def protected(path):
    for directory in [path, *path.parents]:
        info = directory.lstat()
        if (not stat.S_ISDIR(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022):
            raise ValueError("Unsafe installation directory")


def install(path, raw, mode):
    descriptor, temporary = tempfile.mkstemp(prefix=".recorder-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(raw)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary, mode)
        os.link(temporary, path)
        directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        os.unlink(temporary)


def main():
    if len(sys.argv) != 2 or os.geteuid() != 0:
        raise ValueError("Run with sudo and the verified bundle SHA256")
    descriptor = os.open("/tmp/radar-paper-market-bundle.tar", os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError("Expected regular bundle file")
        raw = stream.read(262145)
    if len(raw) > 262144:
        raise ValueError("Recorder bundle too large")
    contents = bundle(raw, sys.argv[1])
    reader = Path("/usr/local/lib/radar-paper/radar")
    protected(reader.parent)
    info = reader.lstat()
    if (not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022
            or hashlib.sha256(reader.read_bytes()).hexdigest() != READER_SHA256):
        raise ValueError("Expected verified installed reader")
    credentials = Path("/etc/radar/alpaca-paper.env")
    protected(credentials.parent)
    info = credentials.lstat()
    if (not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or stat.S_IMODE(info.st_mode) != 0o600):
        raise ValueError("Expected private paper credentials")
    for name, (target, mode) in FILES.items():
        path = Path(target)
        protected(path.parent)
        if path.exists() or path.is_symlink():
            raise ValueError("Recorder already installed; explicit upgrade required")
    for name, (target, mode) in FILES.items():
        install(Path(target), contents[name], mode)
    clean = {"PATH": "/usr/bin:/bin"}
    subprocess.run(["/usr/bin/systemctl", "daemon-reload"], check=True, env=clean)
    # First worker pass must succeed before unattended collection is enabled.
    subprocess.run(["/usr/bin/systemctl", "start", "radar-paper-market.service"], check=True, env=clean)
    subprocess.run(["/usr/bin/systemctl", "enable", "--now", "radar-paper-market.timer"], check=True, env=clean)
    print("IEX research recorder enabled. Storage cap 256 MiB; trading remains disabled.")


if __name__ == "__main__":
    try:
        main()
    except Exception:
        sys.exit("Recorder setup failed; inspect service status. No trading authority enabled.")
