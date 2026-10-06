#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""One-time administrator setup; secrets are entered only on the VPS terminal.

Does not restart Radar, log in to ChatGPT, select budgets, or configure a signer.
The fixed radar-deploy procedure applies the settings after this command exits.
"""

import getpass
import json
import os
from pathlib import Path
import pwd
import re
import shutil
import subprocess
import sys
import tempfile
import time


def refuse(message):
    raise SystemExit(message)


def run(*args):
    subprocess.run(args, check=True)


def install(path, content, mode):
    """Keep root-only backups and replace files atomically in their directory."""
    if path.is_symlink():
        refuse(f"Refusing symlink: {path}")
    if path.exists():
        backup = path.with_name(f"{path.name}.before-private-{time.time_ns()}")
        shutil.copyfile(path, backup)
        os.chmod(backup, 0o600)
    fd, name = tempfile.mkstemp(prefix=".radar-", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            stream.write(content)
        os.chmod(name, mode)
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def cli_files():
    return ('#!/bin/sh\n# SPDX-License-Identifier: Apache-2.0\n'
            'exec /usr/bin/python3 /usr/local/lib/radar/radar-codex-bridge.py "$@"\n')


def main():
    if os.geteuid() != 0:
        refuse("Run with sudo in your own SSH terminal.")
    if len(sys.argv) != 2 or not re.fullmatch(r"[a-zA-Z0-9]{20,40}", sys.argv[1]):
        refuse("Usage: sudo python3 setup-private-connections.py <public Privy app ID>")
    app_id = sys.argv[1]
    env_path = Path("/etc/radar/radar.env")
    if env_path.is_symlink() or not env_path.is_file():
        refuse("Expected an existing regular /etc/radar/radar.env")
    original = env_path.read_text()
    settings = {}
    for line in original.splitlines():
        match = re.fullmatch(r"\s*([A-Z][A-Z0-9_]*)\s*=(.*)", line)
        if match:
            if match[1] in settings:
                refuse(f"Resolve duplicate setting first: {match[1]}")
            settings[match[1]] = match[2].strip().strip("\"'")
    for name in ("RADAR_MODEL_API_KEY", "RADAR_MODEL_OPENAI_KEY", "RADAR_MODEL_DAILY_USD"):
        if settings.get(name):
            refuse(f"Existing {name}: review the active inference configuration first.")
    if settings.get("RADAR_PRIVY_APP_ID") not in (None, "", app_id):
        refuse("A different Privy app is configured; review it before replacing it.")
    for executable in ("/usr/bin/codex", "/usr/bin/node", "/usr/bin/python3"):
        if not os.access(executable, os.X_OK):
            refuse(f"Required executable absent: {executable}")
    account = pwd.getpwnam("radar-agent")
    if account.pw_dir != "/var/lib/radar-agent" or account.pw_uid == 0:
        refuse("radar-agent must have the isolated /var/lib/radar-agent home.")
    pwd.getpwnam("guardian")
    sources = Path(__file__).resolve().parent
    files = {name: (sources / name).read_text() for name in (
        "radar-codex-bridge.py", "radar-codex.socket", "radar-codex.service"
    )}
    # A prior sudo wrapper must be reviewed rather than silently kept or removed.
    if Path("/etc/sudoers.d/radar-agent").exists():
        refuse("Existing /etc/sudoers.d/radar-agent: review it before installing the socket service.")

    updates = {
        "RADAR_MODEL_CODEX": "/usr/local/bin/radar-codex",
        "RADAR_PRIVY_APP_ID": app_id,
        "RADAR_CUSTOMER_ACCESS": "closed",
    }
    if not settings.get("RADAR_PRIVY_APP_SECRET"):
        if not sys.stdin.isatty():
            refuse("Privy secret entry requires an interactive terminal.")
        secret = getpass.getpass("Privy app secret (hidden, stored only on this VPS): ")
        if not secret or any(ord(c) < 32 or ord(c) > 126 for c in secret):
            refuse("Secret must be nonempty printable ASCII on one line.")
        updates["RADAR_PRIVY_APP_SECRET"] = secret

    wrapper = cli_files()
    subprocess.run(["/bin/sh", "-n"], input=wrapper, text=True, check=True)
    for directory in (Path(account.pw_dir), Path(account.pw_dir) / ".codex", Path(account.pw_dir) / "work"):
        if directory.is_symlink():
            refuse(f"Refusing symlink: {directory}")
        directory.mkdir(exist_ok=True)
        os.chown(directory, account.pw_uid, account.pw_gid)
        os.chmod(directory, 0o700)
    library = Path("/usr/local/lib/radar")
    if library.is_symlink():
        refuse(f"Refusing symlink: {library}")
    library.mkdir(mode=0o755, exist_ok=True)
    os.chown(library, 0, 0)
    os.chmod(library, 0o755)
    install(library / "radar-codex-bridge.py", files["radar-codex-bridge.py"], 0o644)
    install(Path("/usr/local/bin/radar-codex"), wrapper, 0o755)
    for name in ("radar-codex.socket", "radar-codex.service"):
        install(Path("/etc/systemd/system") / name, files[name], 0o644)
    run("/usr/bin/systemctl", "daemon-reload")
    run("/usr/bin/systemctl", "enable", "--now", "radar-codex.socket")

    kept = [line for line in original.splitlines() if not any(
        re.match(rf"\s*{name}\s*=", line) for name in updates
    )]
    updated = "\n".join(kept) + "\n" + "\n".join(
        f"{name}={json.dumps(value)}" for name, value in updates.items()
    ) + "\n"
    install(env_path, updated, 0o600)
    print("Setup saved. Customer admission remains closed; inference has no allowance.")
    print("Apply through the verified artifact and fixed radar-deploy procedure.")
    print("Then open /automation as the operator and choose Connect ChatGPT.")


if __name__ == "__main__":
    main()
