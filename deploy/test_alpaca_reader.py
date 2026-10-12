# SPDX-License-Identifier: Apache-2.0
"""Verify staging and fixed isolation arguments without root or a real broker."""

import hashlib
import importlib.util
import os
from pathlib import Path
import stat
import tempfile
import types
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("reader", Path(__file__).with_name("read-alpaca-paper.py"))
reader = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reader)


class ReaderTests(unittest.TestCase):
    def test_exact_verified_bytes_installed_and_tampering_refused(self):
        raw = b"verified reader fixture"
        digest = hashlib.sha256(raw).hexdigest()
        real_stat = Path.lstat

        def root_stat(path):
            info = real_stat(path)
            return types.SimpleNamespace(st_mode=info.st_mode & ~0o022, st_uid=0)

        with tempfile.TemporaryDirectory() as temporary, patch.object(Path, "lstat", root_stat):
            source = Path(temporary) / "upload"
            source.write_bytes(raw)
            directory = Path(temporary) / "protected"
            reader.stage(source, directory, digest)
            target = directory / "radar"
            self.assertEqual(target.read_bytes(), raw)
            self.assertEqual(stat.S_IMODE(os.stat(target).st_mode), 0o755)
            source.write_bytes(b"different")
            with self.assertRaisesRegex(ValueError, "hash mismatch"):
                reader.stage(source, directory, digest)
            self.assertEqual(target.read_bytes(), raw)
            source.write_bytes(raw)
            target.unlink()
            target.symlink_to(Path(temporary) / "never-write")
            with self.assertRaisesRegex(ValueError, "symlink"):
                reader.stage(source, directory, digest)
            self.assertFalse((Path(temporary) / "never-write").exists())
        for value in ["", "a" * 63, "z" * 64, digest + "\n"]:
            with self.assertRaises(ValueError):
                reader.stage("/never-read", Path("/never-write"), value)

    def test_unsafe_parent_refuses(self):
        for mode, uid in [(stat.S_IFDIR | 0o777, 0), (stat.S_IFDIR | 0o755, 1),
                          (stat.S_IFLNK | 0o755, 0)]:
            with patch.object(Path, "lstat", return_value=types.SimpleNamespace(st_mode=mode, st_uid=uid)):
                with self.assertRaises(ValueError):
                    reader.protected_directory(Path("/untrusted"))

    def test_only_isolated_fixed_reader_can_run(self):
        args = reader.command()
        self.assertEqual(args[:7], ["/usr/bin/systemd-run", "--wait", "--pipe", "--collect",
                                   "--service-type=exec", "--unit=radar-alpaca-observe",
                                   "--property=DynamicUser=yes"])
        self.assertEqual(args[-2:], ["/usr/local/lib/radar-paper/radar", "alpaca-read"])
        self.assertIn("--property=EnvironmentFile=/etc/radar/alpaca-paper.env", args)
        self.assertIn("--property=ProtectSystem=strict", args)
        self.assertIn("--property=ProtectHome=yes", args)
        self.assertIn("--property=NoNewPrivileges=yes", args)
        self.assertIn("--property=LimitCORE=0", args)
        self.assertIn("--property=RuntimeMaxSec=45", args)
        self.assertNotIn("alpaca-paper-submit", args)
        with patch.object(reader.os, "geteuid", return_value=1000), patch.object(reader, "stage") as stage:
            with self.assertRaises(ValueError):
                reader.main()
            stage.assert_not_called()

    def test_actual_admin_caller_checks_credentials_before_start(self):
        digest = "a" * 64
        valid = types.SimpleNamespace(st_mode=stat.S_IFREG | 0o600, st_uid=0)
        with patch.object(reader.sys, "argv", ["reader.py", digest]), \
                patch.object(reader.os, "geteuid", return_value=0), \
                patch.object(reader, "protected_directory"), \
                patch.object(Path, "lstat", return_value=valid) as info, \
                patch.object(reader, "stage") as stage, \
                patch.object(reader.subprocess, "run") as run:
            reader.main()
            stage.assert_called_once_with("/tmp/radar-paper-reader.new", Path("/usr/local/lib/radar-paper"), digest)
            run.assert_called_once_with(reader.command(), check=True, env={"PATH": "/usr/bin:/bin"})
            for mode, uid in [(stat.S_IFREG | 0o640, 0), (stat.S_IFREG | 0o600, 1000),
                              (stat.S_IFLNK | 0o600, 0)]:
                stage.reset_mock()
                run.reset_mock()
                info.return_value = types.SimpleNamespace(st_mode=mode, st_uid=uid)
                with self.assertRaises(ValueError):
                    reader.main()
                stage.assert_not_called()
                run.assert_not_called()
