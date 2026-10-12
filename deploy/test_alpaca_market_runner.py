# SPDX-License-Identifier: Apache-2.0
"""Actual capture-to-replay glue, with fake commands and no credentials."""

import importlib.util
import json
from pathlib import Path
import types
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("reader", Path(__file__).with_name("read-alpaca-paper.py"))
reader = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reader)


class MarketRunnerTests(unittest.TestCase):
    def test_capture_then_credential_free_offline_replay_at_original_watermark(self):
        calls = []
        metadata = dict(capture_written=True, feed="iex", execution_authority=False,
                        available_at_unix_ms=100)

        def run(args, **kwargs):
            calls.append((args, kwargs))
            return types.SimpleNamespace(stdout=json.dumps(metadata))

        with patch.object(reader.uuid, "uuid4", return_value=types.SimpleNamespace(hex="a" * 32)), \
                patch("builtins.print") as output:
            reader.observe_market(run)
        self.assertEqual(len(calls), 2)
        capture, replay = calls
        path = "/var/lib/radar-paper-market/capture-" + "a" * 32 + ".json"
        self.assertEqual(capture[0][-6:], ["/usr/local/lib/radar-paper/radar", "alpaca-market-read",
                                         "--symbols", "SPY,QQQ", "--output", path])
        self.assertEqual(replay[0][-6:], ["/usr/local/lib/radar-paper/radar", "alpaca-market-replay",
                                        "--input", path, "--as-of-unix-ms", "100"])
        self.assertIn("--property=EnvironmentFile=/etc/radar/alpaca-paper.env", capture[0])
        self.assertNotIn("--property=EnvironmentFile=/etc/radar/alpaca-paper.env", replay[0])
        self.assertIn("--property=PrivateNetwork=yes", replay[0])
        self.assertIn("--property=UnsetEnvironment=RADAR_ALPACA_PAPER_KEY_ID RADAR_ALPACA_PAPER_SECRET_KEY", replay[0])
        self.assertNotEqual(capture[0][5], replay[0][5])
        for args, kwargs in calls:
            self.assertIn("--property=StateDirectory=radar-paper-market", args)
            self.assertIn("--property=StateDirectoryMode=0700", args)
            self.assertIn("--property=DynamicUser=yes", args)
            self.assertEqual(kwargs["env"], {"PATH": "/usr/bin:/bin"})
            self.assertTrue(kwargs["check"])
            self.assertNotIn("alpaca-paper-submit", args)
        self.assertTrue(capture[1]["capture_output"])
        output.assert_called_once_with(f"Retained market capture: {path}")

    def test_invalid_capture_metadata_never_replays(self):
        valid = dict(capture_written=True, feed="iex", execution_authority=False,
                     available_at_unix_ms=100)
        for key, value in [("capture_written", False), ("feed", "sip"),
                           ("execution_authority", True), ("available_at_unix_ms", None),
                           ("available_at_unix_ms", True), ("available_at_unix_ms", 0),
                           ("available_at_unix_ms", 2**64)]:
            metadata = {**valid, key: value}
            calls = []

            def run(*args, **kwargs):
                calls.append(args)
                return types.SimpleNamespace(stdout=json.dumps(metadata))

            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                reader.observe_market(run)
            self.assertEqual(len(calls), 1)

    def test_paths_modes_and_watermarks_cannot_inject_commands(self):
        for mode, path, watermark in [("submit", "capture-" + "a" * 32 + ".json", 100),
                                     ("market", "../credentials", None),
                                     ("market", "/etc/radar/alpaca-paper.env", None),
                                     ("market", None, None), ("replay", "capture-a.json", 100)]:
            with self.assertRaises(ValueError):
                reader.command(mode, path, watermark)

    def test_actual_market_caller_and_unknown_mode(self):
        info = types.SimpleNamespace(st_mode=0o100600, st_uid=0)
        with patch.object(reader.sys, "argv", ["reader.py", "a" * 64, "--market"]), \
                patch.object(reader.os, "geteuid", return_value=0), \
                patch.object(reader, "protected_directory"), \
                patch.object(Path, "lstat", return_value=info), \
                patch.object(reader, "stage") as stage, \
                patch.object(reader, "observe_market") as observe, \
                patch.object(reader.subprocess, "run") as run:
            reader.main()
            stage.assert_called_once()
            observe.assert_called_once_with(run)
            run.assert_not_called()
            reader.sys.argv[2] = "--submit"
            observe.reset_mock()
            stage.reset_mock()
            with self.assertRaises(ValueError):
                reader.main()
            observe.assert_not_called()
            stage.assert_not_called()
