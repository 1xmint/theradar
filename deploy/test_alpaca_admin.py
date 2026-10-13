# SPDX-License-Identifier: Apache-2.0
"""The administrative grant has fixed actions and no caller-supplied commands."""

import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile
import types
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("paper_admin", Path(__file__).with_name("radar-paper-admin.py"))
admin = importlib.util.module_from_spec(spec)
spec.loader.exec_module(admin)


class AdminTests(unittest.TestCase):
    def test_root_and_exact_action_arguments_refuse_before_dispatch(self):
        for uid, args in ((1, ["admin", "activities"]), (0, ["admin"]),
                          (0, ["admin", "activities", "--url=other"]), (0, ["admin", "shell"])):
            with patch.object(admin.os, "geteuid", return_value=uid), patch.object(admin.sys, "argv", args), \
                    patch.object(admin, "call", side_effect=AssertionError("must not dispatch")), \
                    patch.object(admin, "plan", side_effect=AssertionError("must not read config")), \
                    self.assertRaises(ValueError):
                admin.main()

    def test_configuration_pins_observation_account_and_bounded_pages(self):
        with tempfile.TemporaryDirectory() as temporary:
            config = Path(temporary) / "plan.json"
            plan = dict(version="alpaca-maintenance-v1", account_id=admin.ACCOUNT, mode="observe", max_pages=2)
            with patch.object(admin, "CONFIG", config), patch.object(admin, "protected"):
                for pages in (1, 10):
                    config.write_text(json.dumps({**plan, "max_pages":pages}))
                    self.assertEqual(admin.plan()["max_pages"], pages)
                for field, value in (("account_id", "other"), ("mode", "cancel_working"),
                        ("max_pages", True), ("max_pages", 0), ("max_pages", 11), ("url", "other")):
                    config.write_text(json.dumps({**plan, field:value}))
                    with self.assertRaises(ValueError):
                        admin.plan()
                config.write_text(" " * 65537)
                with self.assertRaises(ValueError):
                    admin.plan()

    def test_protected_paths_refuse_user_owned_writable_or_symlink_inputs(self):
        safe_dir = types.SimpleNamespace(st_mode=stat.S_IFDIR | 0o755, st_uid=0)
        safe_file = types.SimpleNamespace(st_mode=stat.S_IFREG | 0o755, st_uid=0)
        target = Path("/protected/reader")
        for bad in (types.SimpleNamespace(st_mode=stat.S_IFREG | 0o777, st_uid=0),
                    types.SimpleNamespace(st_mode=stat.S_IFREG | 0o755, st_uid=1),
                    types.SimpleNamespace(st_mode=stat.S_IFLNK | 0o755, st_uid=0)):
            with patch.object(Path, "lstat", lambda path: bad if path == target else safe_dir), \
                    self.assertRaises(ValueError):
                admin.protected(target)
        with patch.object(Path, "lstat", lambda path: safe_file if path == target else safe_dir):
            admin.protected(target)
        with patch.object(Path, "lstat", return_value=types.SimpleNamespace(st_mode=stat.S_IFDIR | 0o777, st_uid=0)), \
                self.assertRaises(ValueError):
            admin.protected(target)

    def test_diagnostics_are_fixed_isolated_cli_with_no_order_or_genesis_option(self):
        for action, command in (("activities", "alpaca-activities-read"), ("maintain", "alpaca-paper-maintain")):
            with patch.object(admin.os, "geteuid", return_value=0), patch.object(admin.sys, "argv", ["admin", action]), \
                    patch.object(admin, "plan", return_value={"max_pages":2}), patch.object(admin, "protected"), \
                    patch.object(admin.subprocess, "run") as run:
                admin.main()
                args, kwargs = run.call_args
                argv = args[0]
                self.assertEqual(argv, admin.diagnostic(action, 2))
                self.assertEqual(kwargs, dict(check=True, timeout=100, env={"PATH":"/usr/bin:/bin"}))
                tail = [str(admin.READER), command]
                if action == "maintain":
                    tail += ["--journal", "/var/lib/radar-paper-execution/orders.jsonl"]
                    self.assertIn("StateDirectory=radar-paper-execution", argv)
                    self.assertIn("User=radar-paper-execution", argv)
                else:
                    self.assertNotIn("StateDirectory=radar-paper-execution", argv)
                tail += ["--account-id", admin.ACCOUNT, "--max-pages", "2"]
                self.assertEqual(argv[-len(tail):], tail)
                self.assertEqual(argv[:5], ["/usr/bin/systemd-run", "--wait", "--pipe", "--collect",
                                           "--unit=radar-paper-admin-" + action])
                for property in ("DynamicUser=yes", "EnvironmentFile=/etc/radar/alpaca-paper.env",
                                 "ProtectSystem=strict", "NoNewPrivileges=yes", "CapabilityBoundingSet=",
                                 "MemoryMax=128M", "RuntimeMaxSec=75"):
                    self.assertIn(property, argv)

    def test_timer_start_requires_successful_observation_probe_and_stop_never_kills_worker(self):
        for failed in (False, True):
            calls = []
            def call(argv):
                calls.append(argv)
                if failed:
                    raise subprocess.CalledProcessError(1, argv)
            with patch.object(admin.os, "geteuid", return_value=0), patch.object(admin.sys, "argv", ["admin", "start"]), \
                    patch.object(admin, "plan", return_value={"max_pages":2}), patch.object(admin, "protected"), \
                    patch.object(admin, "call", side_effect=call):
                if failed:
                    with self.assertRaises(subprocess.CalledProcessError):
                        admin.main()
                else:
                    admin.main()
            expected = [["/usr/bin/systemctl", "start", admin.SERVICE]]
            if not failed:
                expected += [["/usr/bin/systemctl", "enable", "--now", admin.TIMER]]
            self.assertEqual(calls, expected)
        with patch.object(admin.os, "geteuid", return_value=0), patch.object(admin.sys, "argv", ["admin", "stop"]), \
                patch.object(admin, "plan", side_effect=AssertionError("stop must work with invalid config")), \
                patch.object(admin, "call") as call:
            admin.main()
            call.assert_called_once_with(["/usr/bin/systemctl", "stop", admin.TIMER])

    def test_inspection_reads_metadata_only_and_cannot_read_credential_contents(self):
        info = types.SimpleNamespace(st_size=123, st_mode=stat.S_IFREG | 0o600)
        with patch.object(admin.os, "geteuid", return_value=0), patch.object(admin.sys, "argv", ["admin", "inspect"]), \
                patch.object(admin, "call") as call, patch.object(Path, "lstat", return_value=info), \
                patch.object(Path, "open", side_effect=AssertionError("must not read any file contents")), \
                patch("builtins.print") as output:
            admin.main()
            self.assertEqual(call.call_count, 2)
            self.assertEqual(call.call_args_list[1].args[0], ["/usr/bin/journalctl", "--unit=" + admin.SERVICE,
                                                           "--lines=12", "--no-pager"])
            self.assertEqual([json.loads(item.args[0])["file"] for item in output.call_args_list],
                             ["orders.jsonl", ".lock", "genesis-complete.json"])
