#!/usr/bin/python3 -I
# SPDX-License-Identifier: Apache-2.0
"""Create supervisor history once, under its service UID, never broker genesis."""

import importlib.util
import os
from pathlib import Path
import sys

spec = importlib.util.spec_from_file_location("paper_shared", Path(__file__).with_name("paper-control.py"))
shared = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shared)
STORE = Path("/var/lib/radar-paper-supervisor")
ACCOUNT = "374d3d8e-63ef-41d1-a1a3-b7921776fbc4"


def provision(store):
    if any(store.iterdir()):
        raise ValueError("Existing supervisor state; inspect, never reinitialize")
    descriptor = os.open(store / ".lock", os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, "wb") as stream:
        stream.write(b"paper-supervisor-origin-started\n")
        stream.flush()
        os.fsync(stream.fileno())
    descriptor = os.open(store, os.O_RDONLY | os.O_DIRECTORY)
    try: os.fsync(descriptor)
    finally: os.close(descriptor)
    # A failed publication retains the fence; it cannot be retried over history.
    shared.publish(store / "origin.json", dict(version="alpaca-paper-supervisor-v1", account_id=ACCOUNT), exclusive=True)


if __name__ == "__main__":
    try:
        if sys.argv[1:] or os.geteuid() == 0:
            raise ValueError("Provision through the supervisor's isolated service identity")
        provision(STORE)
        print('{"supervisor_origin_created":true,"new_order_authority":false}')
    except Exception:
        sys.exit("Supervisor provisioning failed; retain state and inspect before recovery")
