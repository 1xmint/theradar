# SPDX-License-Identifier: Apache-2.0
"""The one-time sudo grant installs only hash-reviewed, fixed-command source."""

from contextlib import contextmanager
import hashlib
import importlib.util
import os
from pathlib import Path
import stat
import tempfile
import types
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("admin_setup", Path(__file__).with_name("setup-alpaca-admin.py"))
setup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(setup)


class AdminSetupTests(unittest.TestCase):
    @contextmanager
    def context(self, invalid_rule=False):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, target, rule = (root / name for name in ("source", "admin", "rule"))
            raw = b"reviewed-helper"
            source.write_bytes(raw)
            calls = []
            actual_run = setup.subprocess.run
            def validate(argv, **kwargs):
                self.assertEqual(argv[:2], ["/usr/sbin/visudo", "-cf"])
                self.assertEqual(kwargs, dict(check=True, env={"PATH":"/usr/bin:/bin"}, capture_output=True))
                self.assertFalse(target.exists())
                self.assertFalse(rule.exists())
                calls.append(Path(argv[2]).read_text())
                if invalid_rule:
                    raise setup.subprocess.CalledProcessError(1, argv)
                # Validate the real sudoers grammar on GitHub without installing it.
                actual_run(argv, **kwargs)
            with patch.object(setup, "SOURCE", source), patch.object(setup, "TARGET", target), \
                    patch.object(setup, "RULE", rule), patch.object(setup, "protected"), \
                    patch.object(setup.os, "geteuid", return_value=0), \
                    patch.object(setup.subprocess, "run", side_effect=validate), patch("builtins.print"):
                yield types.SimpleNamespace(root=root, source=source, target=target, rule=rule,
                    raw=raw, hash=hashlib.sha256(raw).hexdigest(), calls=calls)

    def test_root_valid_hash_and_exact_bytes_are_required(self):
        with self.context() as case:
            for uid, digest in ((1, case.hash), (0, "bad"), (0, "0" * 64)):
                with patch.object(setup.os, "geteuid", return_value=uid), self.assertRaises(ValueError):
                    setup.install(digest)
            self.assertFalse(case.target.exists())
            self.assertEqual(case.calls, [])

    def test_source_is_bounded_regular_and_no_follow(self):
        for kind in ("oversized", "fifo", "symlink"):
            with self.context() as case:
                if kind == "oversized":
                    case.source.write_bytes(b"x" * 65537)
                    digest = hashlib.sha256(case.source.read_bytes()).hexdigest()
                else:
                    case.source.unlink()
                    digest = case.hash
                    if kind == "fifo":
                        os.mkfifo(case.source)
                    else:
                        original = case.root / "original"
                        original.write_bytes(case.raw)
                        case.source.symlink_to(original)
                with self.assertRaises((ValueError, OSError)):
                    setup.install(digest)
                self.assertFalse(case.target.exists())

    def test_existing_targets_and_symlinks_are_never_replaced(self):
        for name in ("target", "rule"):
            for alias in (False, True):
                with self.context() as case:
                    path = getattr(case, name)
                    if alias:
                        path.symlink_to(case.root / "absent")
                    else:
                        path.write_bytes(b"retained")
                    with self.assertRaises(ValueError):
                        setup.install(case.hash)
                    self.assertEqual(case.calls, [])
                    if not alias:
                        self.assertEqual(path.read_bytes(), b"retained")

    def test_visudo_must_succeed_before_any_grant_and_rules_are_exact(self):
        with self.context(invalid_rule=True) as case:
            with self.assertRaises(setup.subprocess.CalledProcessError):
                setup.install(case.hash)
            self.assertFalse(case.target.exists())
            self.assertFalse(case.rule.exists())
        with self.context() as case:
            setup.install(case.hash)
            self.assertEqual(case.target.read_bytes(), case.raw)
            self.assertEqual(stat.S_IMODE(case.target.stat().st_mode), 0o755)
            self.assertEqual(stat.S_IMODE(case.rule.stat().st_mode), 0o440)
            self.assertEqual(case.rule.read_text(), "# Radar paper observation administration only.\n"
                "guardian ALL=(root) NOPASSWD: " + ", ".join(str(case.target) + " " + action
                for action in ("inspect", "activities", "maintain", "stop", "start")) + "\n")
            self.assertEqual(case.calls, [case.rule.read_text()])

    def test_publish_is_exclusive_and_partial_install_never_grants_sudo(self):
        with self.context() as case:
            original = setup.publish
            def failed(path, raw, mode):
                if path == case.rule:
                    raise OSError("fixture write failure")
                original(path, raw, mode)
            with patch.object(setup, "publish", side_effect=failed), self.assertRaises(OSError):
                setup.install(case.hash)
            self.assertTrue(case.target.is_file())
            self.assertFalse(case.rule.exists())
            with self.assertRaises(ValueError):
                setup.install(case.hash)
        with self.context() as case:
            setup.publish(case.target, b"retained", 0o755)
            with self.assertRaises(FileExistsError):
                setup.publish(case.target, b"replacement", 0o755)
            self.assertEqual(case.target.read_bytes(), b"retained")

    def test_protected_ancestors_refuse_nonroot_writable_or_symlink_directories(self):
        for mode, uid in ((stat.S_IFDIR | 0o755, 1), (stat.S_IFDIR | 0o777, 0), (stat.S_IFLNK | 0o755, 0)):
            with patch.object(Path, "lstat", return_value=types.SimpleNamespace(st_mode=mode, st_uid=uid)), \
                    self.assertRaises(ValueError):
                setup.protected(Path("/protected"))
