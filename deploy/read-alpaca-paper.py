#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Install a verified reader and run one isolated observation; never activate trading."""

import hashlib
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile


def protected_directory(path):
    for parent in [path, *path.parents]:
        info = parent.lstat()
        if (not stat.S_ISDIR(info.st_mode) or info.st_uid != 0
                or info.st_mode & 0o022):
            raise ValueError("Reader directories must be root-owned without public write access")


def stage(source, directory, expected):
    if not re.fullmatch(r"[0-9a-f]{64}", expected):
        raise ValueError("Use the reader SHA256 from the verified GitHub build")
    # Read once and install these exact verified bytes. A later replacement of
    # the upload must not substitute a different executable after hashing.
    source_fd = os.open(source, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(source_fd, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError("Expected a regular reader artifact")
        raw = stream.read(256 * 1024 * 1024 + 1)
    if len(raw) > 256 * 1024 * 1024 or hashlib.sha256(raw).hexdigest() != expected:
        raise ValueError("Reader artifact hash mismatch")
    directory = Path(directory)
    protected_directory(directory.parent)
    directory.mkdir(mode=0o755, exist_ok=True)
    protected_directory(directory)
    target = directory / "radar"
    if target.is_symlink():
        raise ValueError("Refusing reader symlink")
    fd, temporary = tempfile.mkstemp(prefix=".reader-", dir=directory)
    try:
        with os.fdopen(fd, "wb") as stream:
            stream.write(raw)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary, 0o755)
        os.replace(temporary, target)
        sync_fd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(sync_fd)
        finally:
            os.close(sync_fd)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def command():
    # The administrator explicitly starts this one observation. No timer,
    # listener, enable command, sudo rule or arbitrary CLI arguments are added.
    properties = [
        "DynamicUser=yes", "EnvironmentFile=/etc/radar/alpaca-paper.env",
        "NoNewPrivileges=yes", "ProtectSystem=strict", "ProtectHome=yes",
        "PrivateTmp=yes", "PrivateDevices=yes", "ProtectProc=invisible",
        "ProtectKernelTunables=yes", "ProtectKernelModules=yes",
        "ProtectControlGroups=yes", "RestrictSUIDSGID=yes",
        "CapabilityBoundingSet=", "LimitCORE=0", "MemoryMax=256M",
        "TasksMax=16", "RuntimeMaxSec=45",
        "RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6",
    ]
    return ["/usr/bin/systemd-run", "--wait", "--pipe", "--collect",
            "--service-type=exec", "--unit=radar-alpaca-observe",
            *[f"--property={value}" for value in properties],
            "/usr/local/lib/radar-paper/radar", "alpaca-read"]


def main():
    if len(sys.argv) != 2 or os.geteuid() != 0:
        raise ValueError("Run with sudo and the verified reader SHA256")
    credentials = Path("/etc/radar/alpaca-paper.env")
    protected_directory(credentials.parent)
    info = credentials.lstat()
    if (not stat.S_ISREG(info.st_mode) or info.st_uid != 0
            or stat.S_IMODE(info.st_mode) != 0o600):
        raise ValueError("Expected root-only provisioned paper credentials")
    stage("/tmp/radar-paper-reader.new", Path("/usr/local/lib/radar-paper"), sys.argv[1])
    # Never read the credential file into this installer or the SSH user's
    # environment. systemd passes it only to the distinct temporary reader UID.
    subprocess.run(command(), check=True, env={"PATH": "/usr/bin:/bin"})


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError):
        sys.exit("Isolated paper observation failed. Trading remains disabled.")
