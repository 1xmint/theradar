# SPDX-License-Identifier: Apache-2.0
"""Exercise research setup on temporary paths; never call systemd or read keys."""

import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import stat
import tarfile
import tempfile
import types
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("setup", Path(__file__).with_name("setup-alpaca-context.py"))
setup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(setup)


class ContextSetupTests(unittest.TestCase):
    def plan(self, start=121001):
        return {"start_unix_ms":start,"slots":12,"interval_ms":600000,
                "experiment":{"strategy_id":"snapshot-context-v1","horizon_ms":600000}}

    def test_schedule_requires_future_margin_and_bounds_distant_starts(self):
        now = 1000
        setup.validate_schedule(self.plan(now + 120001), now)
        setup.validate_schedule(self.plan(now + 7 * 86400000), now)
        for start in (now, now + 120000, now + 7 * 86400000 + 1, True, None, "121001", 121001.0):
            with self.subTest(start=start), self.assertRaises(ValueError):
                setup.validate_schedule(self.plan(start), now)

    def test_schedule_requires_context_strategy_and_fixed_cohort_shape(self):
        for changes in ({"slots":13}, {"slots":12.0}, {"interval_ms":600000.0},
                        {"interval_ms":120000}, {"experiment":{}}, {"experiment":None},
                        {"experiment":{"strategy_id":"other","horizon_ms":600000}},
                        {"experiment":{"strategy_id":"snapshot-context-v1","horizon_ms":600000.0}},
                        {"experiment":{"strategy_id":"snapshot-context-v1","horizon_ms":1}}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                setup.validate_schedule(self.plan() | changes, 1000)

    def test_context_targets_are_disjoint_from_existing_research_installation(self):
        self.assertEqual(setup.LIBRARY, "/usr/local/lib/radar-context-research")
        self.assertEqual(setup.FILES["alpaca-context-research.json.example"][0],
                         "/etc/radar/alpaca-context-research.json")
        self.assertEqual(setup.FILES["radar-context-research.service"][0],
                         "/etc/systemd/system/radar-context-research.service")
        self.assertFalse(any("/radar-research/" in path or "radar-paper-research" in path
                             for path, mode in setup.FILES.values()))

    def archive(self, names=None, link=False):
        output = io.BytesIO()
        with tarfile.open(fileobj=output, mode="w") as archive:
            for name in list(setup.FILES) if names is None else names:
                item = tarfile.TarInfo(name)
                raw = (b'{"version":"alpaca-research-v1","start_unix_ms":121001,"slots":12,"interval_ms":600000,"experiment":{"strategy_id":"snapshot-context-v1","horizon_ms":600000}}'
                       if name == "alpaca-context-research.json.example" else b"reviewed-fixture")
                item.size = len(raw)
                if link:
                    item.type = tarfile.SYMTYPE
                    item.linkname = "/etc/passwd"
                archive.addfile(item, io.BytesIO(raw))
        return output.getvalue()

    def test_bundle_requires_exact_hash_members_and_regular_files(self):
        raw = self.archive()
        self.assertEqual(set(setup.bundle(raw, hashlib.sha256(raw).hexdigest())), set(setup.FILES))
        with self.assertRaises(ValueError):
            setup.bundle(raw, "0" * 64)
        for raw in (self.archive([]), self.archive(["../escape"]), self.archive(link=True),
                    self.archive(list(setup.FILES) + [next(iter(setup.FILES))])):
            with self.assertRaises(ValueError):
                setup.bundle(raw, hashlib.sha256(raw).hexdigest())

    def test_install_cannot_overwrite(self):
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary) / "binary"
            setup.install(target, b"original", 0o755)
            self.assertEqual(stat.S_IMODE(target.stat().st_mode), 0o755)
            with self.assertRaises(FileExistsError):
                setup.install(target, b"replacement", 0o755)
            self.assertEqual(target.read_bytes(), b"original")
            self.assertEqual(list(target.parent.iterdir()), [target])

    def test_protected_rejects_mutable_nonroot_and_symlink_directories(self):
        for mode, uid in ((stat.S_IFDIR | 0o777, 0), (stat.S_IFDIR | 0o755, 1), (stat.S_IFLNK | 0o755, 0)):
            with patch.object(Path, "lstat", return_value=types.SimpleNamespace(st_mode=mode, st_uid=uid)):
                with self.assertRaises(ValueError):
                    setup.protected(Path("/unsafe"))

    def test_owner_only_entry_refuses_before_reading(self):
        with patch.object(setup.os, "geteuid", return_value=1), patch.object(setup.sys, "argv", ["setup", "0" * 64]):
            with self.assertRaises(ValueError), patch.object(setup, "read", side_effect=AssertionError("read")):
                setup.main()

    def test_actual_setup_hashes_permissions_probe_order_and_no_overwrite(self):
        for bad_reader, mode, failed_service in ((False, 0o600, None), (True, 0o600, None),
                                                (False, 0o640, None), (False, 0o600, "start")):
            with self.subTest(bad_reader=bad_reader, mode=mode, failed_service=failed_service), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                library = root / "library"
                credentials = root / "credentials"
                credentials.write_text("secret-not-read")
                credentials.chmod(mode)
                raw = self.archive()
                files = {name: (str(library / name) if name.endswith(".py") else str(root / name), filemode)
                         for name, (_, filemode) in setup.FILES.items()}
                original_stat = Path.lstat
                def root_stat(path):
                    info = original_stat(path)
                    return types.SimpleNamespace(st_mode=info.st_mode, st_uid=0)
                def path(value):
                    return credentials if str(value) == "/etc/radar/alpaca-paper.env" else Path(value)
                def read(value, limit):
                    return raw if str(value).endswith(".tar") else b"wrong" if bad_reader else b"verified-reader"
                calls = []
                def run(argv, **kwargs):
                    self.assertEqual(kwargs, dict(check=True, env={"PATH": "/usr/bin:/bin"}))
                    calls.append(argv)
                    if argv[1] == failed_service:
                        raise setup.subprocess.CalledProcessError(1, argv)
                with patch.object(setup.sys, "argv", ["setup", hashlib.sha256(raw).hexdigest()]), \
                        patch.object(setup.os, "geteuid", return_value=0), patch.object(setup, "read", side_effect=read), \
                        patch.object(setup, "Path", side_effect=path), patch.object(Path, "lstat", root_stat), \
                        patch.object(setup, "protected"), patch.object(setup, "FILES", files), \
                        patch.object(setup, "LIBRARY", str(library)), \
                        patch.object(setup, "READER_SHA256", hashlib.sha256(b"verified-reader").hexdigest()), \
                        patch.object(setup.time, "time_ns", return_value=1000000000), \
                        patch.object(setup.subprocess, "run", side_effect=run), patch("builtins.print"):
                    if bad_reader or mode != 0o600:
                        with self.assertRaises(ValueError):
                            setup.main()
                        self.assertFalse(calls)
                        self.assertFalse(library.exists())
                    elif failed_service:
                        with self.assertRaises(setup.subprocess.CalledProcessError):
                            setup.main()
                        self.assertFalse(any(call[1] == "enable" for call in calls))
                    else:
                        setup.main()
                        self.assertEqual(calls, [["/usr/bin/systemctl", "is-active", "--quiet", "radar-codex.socket"],
                            ["/usr/bin/systemctl", "daemon-reload"], ["/usr/bin/systemctl", "start", "radar-context-research.service"],
                            ["/usr/bin/systemctl", "enable", "--now", "radar-context-research.timer"]])
                        config = json.loads((root / "alpaca-context-research.json.example").read_text())
                        self.assertEqual(config["start_unix_ms"], 121001)
                        self.assertEqual((library / "radar").read_bytes(), b"verified-reader")
                        calls.clear()
                        with self.assertRaises(ValueError):
                            setup.main()
                        self.assertEqual(len(calls), 1)
