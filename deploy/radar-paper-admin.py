#!/usr/bin/python3 -I
# SPDX-License-Identifier: Apache-2.0
"""Fixed private paper diagnostics and observation lifecycle; no arbitrary root command."""

import json
import os
from pathlib import Path
import stat
import subprocess
import sys

ACCOUNT = "374d3d8e-63ef-41d1-a1a3-b7921776fbc4"
READER = Path("/usr/local/lib/radar-paper-execution/radar")
CONFIG = Path("/etc/radar/alpaca-maintenance.json")
TIMER = "radar-paper-maintenance.timer"
SERVICE = "radar-paper-maintenance.service"
ACTIONS = ("inspect", "activities", "maintain", "stop", "start")


def protected(path):
    for parent in path.parents:
        info = parent.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
            raise ValueError("Unprotected administrative path")
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
        raise ValueError("Unprotected administrative file")


def plan():
    protected(CONFIG)
    with CONFIG.open("rb") as stream:
        raw = stream.read(65537)
    if len(raw) > 65536:
        raise ValueError("Configuration exceeds bound")
    value = json.loads(raw)
    if (not isinstance(value, dict) or set(value) != {"version", "account_id", "max_pages", "mode"}
            or value["version"] != "alpaca-maintenance-v1" or value["account_id"] != ACCOUNT
            or value["mode"] != "observe" or type(value["max_pages"]) is not int
            or not 1 <= value["max_pages"] <= 10):
        raise ValueError("Expected pinned observation configuration")
    return value


def call(argv):
    return subprocess.run(argv, check=True, timeout=100, env={"PATH": "/usr/bin:/bin"})


def diagnostic(action, pages):
    argv = ["/usr/bin/systemd-run", "--wait", "--pipe", "--collect",
            "--unit=radar-paper-admin-" + action]
    properties = ["DynamicUser=yes", "EnvironmentFile=/etc/radar/alpaca-paper.env",
                  "NoNewPrivileges=yes", "ProtectSystem=strict", "ProtectHome=yes",
                  "PrivateTmp=yes", "PrivateDevices=yes", "ProtectProc=invisible",
                  "ProtectKernelTunables=yes", "ProtectKernelModules=yes", "ProtectControlGroups=yes",
                  "RestrictSUIDSGID=yes", "RestrictNamespaces=yes", "LockPersonality=yes",
                  "MemoryDenyWriteExecute=yes", "CapabilityBoundingSet=", "LimitCORE=0",
                  "RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6", "MemoryMax=128M",
                  "TasksMax=16", "RuntimeMaxSec=75", "WorkingDirectory=/", "UMask=0077"]
    if action == "maintain":
        properties += ["User=radar-paper-execution", "StateDirectory=radar-paper-execution",
                       "StateDirectoryMode=0700"]
    for value in properties:
        argv += ["--property", value]
    argv += [str(READER), "alpaca-paper-maintain" if action == "maintain" else "alpaca-activities-read"]
    if action == "maintain":
        argv += ["--journal", "/var/lib/radar-paper-execution/orders.jsonl"]
    return argv + ["--account-id", ACCOUNT, "--max-pages", str(pages)]


def main():
    if os.geteuid() != 0 or len(sys.argv) != 2 or sys.argv[1] not in ACTIONS:
        raise ValueError("Use one fixed paper administrative action with sudo")
    action = sys.argv[1]
    if action == "stop":
        # Stop scheduling; let an already-running observation finish normally.
        call(["/usr/bin/systemctl", "stop", TIMER])
    elif action == "inspect":
        call(["/usr/bin/systemctl", "show", "radar-paper-initialize.service", SERVICE, TIMER,
              "--property=LoadState,ActiveState,SubState,Result,ExecMainStatus"])
        call(["/usr/bin/journalctl", "--unit=" + SERVICE, "--lines=12", "--no-pager"])
        for name in ("orders.jsonl", ".lock", "genesis-complete.json"):
            path = Path("/var/lib/private/radar-paper-execution") / name
            info = path.lstat()
            print(json.dumps({"file":name, "bytes":info.st_size, "regular":stat.S_ISREG(info.st_mode)}))
    else:
        value = plan()
        protected(READER)
        if action == "start":
            for name in (SERVICE, TIMER):
                protected(Path("/etc/systemd/system") / name)
            protected(READER.with_name("maintain-alpaca-paper.py"))
            call(["/usr/bin/systemctl", "start", SERVICE])
            call(["/usr/bin/systemctl", "enable", "--now", TIMER])
        else:
            call(diagnostic(action, value["max_pages"]))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        sys.exit("Paper administration failed; retain history and inspect the fixed command output")
