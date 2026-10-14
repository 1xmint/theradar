#!/usr/bin/python3 -I
# SPDX-License-Identifier: Apache-2.0
"""Fixed paper timer lifecycle and selected service diagnostics; no policy changes."""

import os
from pathlib import Path
import stat
import subprocess
import sys

ACTIONS = ("inspect", "start", "stop")
TIMER = "radar-paper-supervisor.timer"
SERVICE = "radar-paper-supervisor.service"


def call(argv):
    return subprocess.run(argv, check=True, timeout=100, env={"PATH":"/usr/bin:/bin"})


def protected(path):
    for parent in path.parents:
        info = parent.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
            raise ValueError("Unprotected paper administration")
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
        raise ValueError("Unprotected paper administration")


def main():
    if os.geteuid() != 0 or len(sys.argv) != 2 or sys.argv[1] not in ACTIONS:
        raise ValueError("Use one fixed paper autonomy administration operation")
    action = sys.argv[1]
    if action == "inspect":
        call(["/usr/bin/systemctl", "show", SERVICE, TIMER, "radar-paper-control.socket", "radar-paper-control.service",
            "--property=LoadState,ActiveState,SubState,Result,ExecMainStatus,User,Group,SupplementaryGroups"])
        call(["/usr/bin/journalctl", "--unit=" + SERVICE, "--lines=12", "--no-pager"])
    elif action == "stop":
        # Scheduling stops; an in-flight tick is not canceled. Owner pause on
        # the site changes policy; this helper cannot grant or revoke policy.
        call(["/usr/bin/systemctl", "stop", TIMER])
    else:
        for path in ("/etc/systemd/system/" + SERVICE, "/etc/systemd/system/" + TIMER,
                "/usr/local/lib/radar-paper-control/paper-supervisor.py", "/usr/local/lib/radar-paper-control/paper-control.py",
                "/etc/radar/alpaca-paper-supervisor.json", "/etc/radar/alpaca-paper-control.json"):
            protected(Path(path))
        call(["/usr/bin/systemctl", "enable", "--now", TIMER])


if __name__ == "__main__":
    try:
        main()
    except Exception:
        sys.exit("Paper autonomy administration failed; inspect retained state")
