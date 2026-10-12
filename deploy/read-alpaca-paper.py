#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Install a verified reader and run one isolated observation; never activate trading."""

import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile
import uuid


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


def command(mode="account", capture=None, watermark=None):
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
    unit = "radar-alpaca-observe"
    arguments = ["/usr/local/lib/radar-paper/radar", "alpaca-read"]
    if mode != "account":
        if (mode not in ("market", "replay") or not isinstance(capture, str)
                or not re.fullmatch(r"capture-[0-9a-f]{32}\.json", capture)):
            raise ValueError("Invalid market observation mode or filename")
        path = f"/var/lib/radar-paper-market/{capture}"
        properties += ["StateDirectory=radar-paper-market", "StateDirectoryMode=0700"]
        if mode == "market":
            unit = "radar-alpaca-market"
            arguments = ["/usr/local/lib/radar-paper/radar", "alpaca-market-read",
                         "--symbols", "SPY,QQQ", "--output", path]
        else:
            if type(watermark) is not int or not 0 < watermark <= 2**64 - 1:
                raise ValueError("Invalid market availability watermark")
            unit = "radar-alpaca-replay"
            properties.remove("EnvironmentFile=/etc/radar/alpaca-paper.env")
            properties.append("UnsetEnvironment=RADAR_ALPACA_PAPER_KEY_ID RADAR_ALPACA_PAPER_SECRET_KEY")
            properties.append("PrivateNetwork=yes")
            arguments = ["/usr/local/lib/radar-paper/radar", "alpaca-market-replay",
                         "--input", path, "--as-of-unix-ms", str(watermark)]
    return ["/usr/bin/systemd-run", "--wait", "--pipe", "--collect",
            "--service-type=exec", f"--unit={unit}",
            *[f"--property={value}" for value in properties], *arguments]


def observe_market(run):
    # Fresh names preserve captures across retries/restarts, never overwriting
    # earlier observations. The existing CLI publishes a complete synced file.
    capture = f"capture-{uuid.uuid4().hex}.json"
    result = run(command("market", capture), check=True, capture_output=True,
                 text=True, env={"PATH": "/usr/bin:/bin"})
    metadata = json.loads(result.stdout)
    if (not isinstance(metadata, dict) or metadata.get("capture_written") is not True
            or metadata.get("feed") != "iex" or metadata.get("execution_authority") is not False):
        raise ValueError("Invalid market capture metadata")
    replay = command("replay", capture, metadata.get("available_at_unix_ms"))
    # The second process receives no broker credentials and has no network.
    # Replay stays at original availability; this is not a fresh execution quote.
    run(replay, check=True, env={"PATH": "/usr/bin:/bin"})
    print(f"Retained market capture: /var/lib/radar-paper-market/{capture}")


def main():
    if (len(sys.argv) not in (2, 3) or os.geteuid() != 0
            or (len(sys.argv) == 3 and sys.argv[2] != "--market")):
        raise ValueError("Run with sudo and the verified reader SHA256, optionally --market")
    credentials = Path("/etc/radar/alpaca-paper.env")
    protected_directory(credentials.parent)
    info = credentials.lstat()
    if (not stat.S_ISREG(info.st_mode) or info.st_uid != 0
            or stat.S_IMODE(info.st_mode) != 0o600):
        raise ValueError("Expected root-only provisioned paper credentials")
    stage("/tmp/radar-paper-reader.new", Path("/usr/local/lib/radar-paper"), sys.argv[1])
    # Never read the credential file into this installer or the SSH user's
    # environment. systemd passes it only to the distinct temporary reader UID.
    if len(sys.argv) == 3:
        observe_market(subprocess.run)
    else:
        subprocess.run(command(), check=True, env={"PATH": "/usr/bin:/bin"})


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError):
        sys.exit("Isolated paper observation failed. Trading remains disabled.")
