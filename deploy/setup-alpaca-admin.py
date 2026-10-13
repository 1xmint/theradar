#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""One-time root installation of hash-reviewed fixed paper administration."""

import hashlib
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile

SOURCE = Path("/tmp/radar-paper-admin.py")
TARGET = Path("/usr/local/sbin/radar-paper-admin")
RULE = Path("/etc/sudoers.d/radar-paper-admin")
ACTIONS = ("inspect", "activities", "maintain", "stop", "start")


def protected(directory):
    for path in (directory, *directory.parents):
        info = path.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
            raise ValueError("Unprotected administrative installation directory")


def publish(path, raw, mode):
    descriptor, temporary = tempfile.mkstemp(prefix=".radar-admin-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(raw)
            stream.flush()
            os.fchmod(stream.fileno(), mode)
            os.fsync(stream.fileno())
        os.link(temporary, path)
        descriptor = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)
    finally:
        os.unlink(temporary)


def install(expected):
    if os.geteuid() != 0 or not re.fullmatch(r"[0-9a-f]{64}", expected):
        raise ValueError("Use sudo with the verified helper SHA256")
    descriptor = os.open(SOURCE, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError("Expected regular administrative source")
        raw = stream.read(65537)
    if len(raw) > 65536 or hashlib.sha256(raw).hexdigest() != expected:
        raise ValueError("Administrative source hash differs")
    for path in (TARGET, RULE):
        protected(path.parent)
        if path.exists() or path.is_symlink():
            raise ValueError("Administrative target exists; explicit upgrade required")
    rule = ("# Radar paper observation administration only.\n"
            "guardian ALL=(root) NOPASSWD: " + ", ".join(str(TARGET) + " " + action for action in ACTIONS) + "\n").encode()
    descriptor, temporary = tempfile.mkstemp(prefix=".radar-admin-check-", dir=RULE.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(rule)
            stream.flush()
        subprocess.run(["/usr/sbin/visudo", "-cf", temporary], check=True,
                       env={"PATH": "/usr/bin:/bin"}, capture_output=True)
    finally:
        os.unlink(temporary)
    publish(TARGET, raw, 0o755)
    publish(RULE, rule, 0o440)
    print("Fixed paper administration installed. No trading authority changed.")


if __name__ == "__main__":
    try:
        if len(sys.argv) != 2:
            raise ValueError("One reviewed hash required")
        install(sys.argv[1])
    except Exception:
        sys.exit("Administrative setup failed; inspect retained files before any retry")
