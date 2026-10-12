#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Private terminal provisioning only; no orders, service changes or activation."""

import getpass
import json
import os
from pathlib import Path
import re
import stat
import sys
import tempfile
import urllib.request


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ValueError("Redirect refused")


def credential(value):
    if not re.fullmatch(r"[A-Za-z0-9]{16,128}", value):
        raise ValueError("Invalid credential format; enter the paper credential only")
    return value


def verify(key, secret, open_request):
    request = urllib.request.Request(
        "https://paper-api.alpaca.markets/v2/account",
        headers={"APCA-API-KEY-ID": key, "APCA-API-SECRET-KEY": secret},
        method="GET",
    )
    try:
        with open_request(request, timeout=10) as response:
            if response.status != 200:
                raise ValueError()
            raw = response.read(65537)
            if len(raw) > 65536:
                raise ValueError()
            account = json.loads(raw)
        if (not isinstance(account, dict)
                or not isinstance(account.get("id"), str) or not account["id"].strip()
                or account.get("status") != "ACTIVE" or account.get("currency") != "USD"
                or any(account.get(flag) is not False for flag in (
                    "trading_blocked", "account_blocked", "trade_suspended_by_user"))):
            raise ValueError()
    except Exception:
        # Provider bodies and exceptions may contain credentials or private data.
        raise ValueError("Paper account verification failed; nothing saved") from None


def save(directory, key, secret):
    # The protected directory must already exist. Never follow symlinks or
    # overwrite credentials; rotation needs its own explicit operator procedure.
    directory = Path(directory)
    for parent in [directory, *directory.parents]:
        info = parent.lstat()
        if (not stat.S_ISDIR(info.st_mode) or info.st_uid != 0
                or info.st_mode & 0o022):
            raise ValueError("Expected a root-owned directory without public write access")
    target = directory / "alpaca-paper.env"
    fd, temporary = tempfile.mkstemp(prefix=".alpaca-paper-", dir=directory)
    try:
        with os.fdopen(fd, "w", encoding="ascii") as stream:
            stream.write(f"RADAR_ALPACA_PAPER_KEY_ID={credential(key)}\n")
            stream.write(f"RADAR_ALPACA_PAPER_SECRET_KEY={credential(secret)}\n")
            stream.flush()
            os.fsync(stream.fileno())
        # mkstemp is mode 0600. link is atomic and refuses an existing target,
        # including a symlink; there is no check-then-overwrite window.
        os.link(temporary, target)
    finally:
        os.unlink(temporary)


def main():
    if sys.argv[1:] or os.geteuid() != 0 or not sys.stdin.isatty():
        raise ValueError("Run with sudo, without arguments, in your own SSH terminal")
    key = credential(getpass.getpass("Alpaca PAPER key ID (hidden): "))
    secret = credential(getpass.getpass("Alpaca PAPER secret (hidden): "))
    # Ignore ambient proxies as well as refusing redirects: custom auth headers
    # are sent only to the fixed HTTPS paper endpoint.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    verify(key, secret, opener.open)
    save("/etc/radar", key, secret)
    print("Paper account verified. Saved root-only /etc/radar/alpaca-paper.env.")
    print("No service changed; trading remains disabled.")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError):
        sys.exit("Setup refused. Check paper credentials, account status and protected file permissions. Existing credentials are never overwritten.")
