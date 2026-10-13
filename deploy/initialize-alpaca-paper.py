#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Deliberate first checkpoint in fresh private storage, with a durable no-retry fence."""

import fcntl
import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import time

spec = importlib.util.spec_from_file_location("maintenance", Path(__file__).with_name("maintain-alpaca-paper.py"))
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)


def sync(directory):
    descriptor = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def publish(path, value):
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, "wb") as stream:
        stream.write(json.dumps(value, sort_keys=True).encode())
        stream.flush()
        os.fsync(stream.fileno())
    sync(path.parent)


def initialize(directory, config, run, environment, load=worker.configuration, clock=lambda: int(time.time())):
    plan = load(config)
    if plan["mode"] != "observe" or any(directory.iterdir()):
        raise ValueError("Initialization requires observation mode and fresh private storage")
    descriptor = os.open(directory / ".lock", os.O_CREAT | os.O_EXCL | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, "wb") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        # This fence survives success, timeout and crash. Never retry or remove it.
        lock.write(json.dumps({"phase":"genesis_started", "account_id":plan["account_id"]}).encode())
        lock.flush()
        os.fsync(lock.fileno())
        sync(directory)
        journal = directory / "orders.jsonl"
        before = clock()
        result = run([worker.READER, "alpaca-cash-checkpoint", "--journal", str(journal),
                      "--account-id", plan["account_id"], "--max-pages", str(plan["max_pages"])],
                     check=True, capture_output=True, text=True, timeout=75,
                     env=worker.broker_environment(environment))
        finished = clock()
        if len(result.stdout) > 4 * 1024 * 1024:
            raise ValueError("Initialization response exceeds limit")
        report = json.loads(result.stdout)
        if not isinstance(report, dict):
            raise ValueError("Invalid initialization response")
        checkpoint = report.get("checkpoint")
        claims = report.get("claims")
        if (type(report.get("version")) is not int or report["version"] != 1
                or report.get("venue") != "alpaca" or report.get("environment") != "paper"
                or report.get("expected_account_id") != plan["account_id"]
                or report.get("checkpoint_written") is not True
                or report.get("execution_authority") is not False
                or report.get("retry_authorized") is not False
                or report.get("claim_release_authorized") is not False
                or not isinstance(checkpoint, dict) or checkpoint.get("account_id") != plan["account_id"]
                or checkpoint.get("orders") != {} or not isinstance(claims, dict) or claims.get("claims") != []):
            raise ValueError("Initialization did not establish an empty pinned checkpoint")
        started = checkpoint.get("started")
        completed = checkpoint.get("completed")
        if (type(started) is not int or type(completed) is not int
                or not 0 < before <= started <= completed <= finished):
            raise ValueError("Initialization checkpoint times differ from this invocation")
        info = journal.lstat()
        if not stat.S_ISREG(info.st_mode) or not 0 < info.st_size <= 4 * 1024 * 1024:
            raise ValueError("Initialization journal was not retained")
        outcome = {"genesis":"initialized", "account_id":plan["account_id"], "new_order_authority":False}
        publish(directory / "genesis-complete.json", outcome)
        return outcome


def main():
    if sys.argv[1:]:
        raise ValueError("Initialization takes no arguments")
    print(json.dumps(initialize(worker.STORE, worker.CONFIG, subprocess.run, os.environ), sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        sys.exit("Paper initialization failed; inspect retained history, do not retry or delete it")
