# SPDX-License-Identifier: Apache-2.0
"""Fresh-install boundary tests use temporary paths and fake systemd calls."""

from contextlib import contextmanager
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

spec = importlib.util.spec_from_file_location("execution_setup", Path(__file__).with_name("setup-alpaca-execution.py"))
setup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(setup)
ACCOUNT = "374d3d8e-63ef-41d1-a1a3-b7921776fbc4"


class ExecutionSetupTests(unittest.TestCase):
    def plan(self):
        return dict(version="alpaca-maintenance-v1", account_id=ACCOUNT, max_pages=2, mode="observe")

    def archive(self, names=None, link=False):
        output = io.BytesIO()
        with tarfile.open(fileobj=output, mode="w") as archive:
            for name in list(setup.FILES) if names is None else names:
                item = tarfile.TarInfo(name)
                raw = json.dumps(self.plan()).encode() if name.endswith(".example") else b"reviewed-fixture"
                item.size = len(raw)
                if link:
                    item.type = tarfile.SYMTYPE
                    item.linkname = "/etc/passwd"
                archive.addfile(item, io.BytesIO(raw))
        return output.getvalue()

    @contextmanager
    def context(self, bad_reader=False, mode=0o600, uid=0, failed=None):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            library = root / "library"
            credentials = root / "credentials"
            credentials.write_text("private-fixture-never-read")
            credentials.chmod(mode)
            raw = self.archive()
            files = {name: (str(library / name) if name.endswith(".py") else str(root / name), filemode)
                     for name, (_, filemode) in setup.FILES.items()}
            states = (str(root / "state"), str(root / "private-state"))
            original_stat = Path.lstat
            def root_stat(path):
                info = original_stat(path)
                return types.SimpleNamespace(st_mode=info.st_mode, st_uid=uid if path == credentials else 0)
            def path(value):
                mapping = {"/etc/radar/alpaca-paper.env":credentials, "/var/lib":root,
                           "/var/lib/private":root / "private-parent"}
                return mapping.get(str(value), Path(value))
            inputs = []
            def read(value, limit):
                inputs.append(str(value))
                if str(value).endswith(".tar"):
                    return raw
                self.assertEqual(str(value), "/tmp/radar-paper-execution.new")
                return b"wrong" if bad_reader else b"verified-reader"
            calls = []
            def run(argv, **kwargs):
                self.assertEqual(kwargs, dict(check=True, env={"PATH":"/usr/bin:/bin"}))
                calls.append(argv)
                if len(argv) == 3 and argv[1] == "start" and argv[2] == failed:
                    raise setup.subprocess.CalledProcessError(1, argv)
            with patch.object(setup.sys, "argv", ["setup", hashlib.sha256(raw).hexdigest()]), \
                    patch.object(setup.os, "geteuid", return_value=0), patch.object(setup, "read", side_effect=read), \
                    patch.object(setup, "Path", side_effect=path), patch.object(Path, "lstat", root_stat), \
                    patch.object(setup, "protected"), patch.object(setup, "FILES", files), \
                    patch.object(setup, "LIBRARY", str(library)), patch.object(setup, "STATES", states), \
                    patch.object(setup, "READER_SHA256", hashlib.sha256(b"verified-reader").hexdigest()), \
                    patch.object(setup.subprocess, "run", side_effect=run), patch("builtins.print"):
                yield types.SimpleNamespace(root=root, library=library, credentials=credentials,
                    states=[Path(value) for value in states], files=files, calls=calls, inputs=inputs)

    def test_configuration_is_exact_observation_only_and_has_bounded_pages(self):
        for pages in (1, 10):
            setup.validate({**self.plan(), "max_pages":pages})
        for field, value in (("version", "other"), ("mode", "cancel_working"), ("max_pages", 0),
                             ("max_pages", 11), ("max_pages", True), ("max_pages", 1.0),
                             ("account_id", "paper"), ("account_id", ACCOUNT.upper()),
                             ("url", "https://api.alpaca.markets")):
            with self.assertRaises(ValueError):
                setup.validate({**self.plan(), field:value})

    def test_bundle_hash_members_duplicates_and_links_are_checked(self):
        raw = self.archive()
        self.assertEqual(set(setup.bundle(raw, hashlib.sha256(raw).hexdigest())), set(setup.FILES))
        with self.assertRaises(ValueError):
            setup.bundle(raw, "0" * 64)
        for raw in (self.archive([]), self.archive(["../escape"]), self.archive(link=True),
                    self.archive(list(setup.FILES) + [next(iter(setup.FILES))])):
            with self.assertRaises(ValueError):
                setup.bundle(raw, hashlib.sha256(raw).hexdigest())

    def test_exclusive_install_preserves_files_and_symlink_targets(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / "target"
            setup.install(target, b"original", 0o755)
            self.assertEqual(stat.S_IMODE(target.stat().st_mode), 0o755)
            with self.assertRaises(FileExistsError):
                setup.install(target, b"replacement", 0o755)
            alias = root / "alias"
            alias.symlink_to(target)
            with self.assertRaises(FileExistsError):
                setup.install(alias, b"replacement", 0o755)
            self.assertEqual(target.read_bytes(), b"original")
            self.assertEqual(set(root.iterdir()), {target, alias})

    def test_protected_ancestors_reject_nonroot_writable_and_symlink_directories(self):
        for mode, uid in ((stat.S_IFDIR | 0o777, 0), (stat.S_IFDIR | 0o755, 1), (stat.S_IFLNK | 0o755, 0)):
            with patch.object(Path, "lstat", return_value=types.SimpleNamespace(st_mode=mode, st_uid=uid)), \
                    self.assertRaises(ValueError):
                setup.protected(Path("/unsafe"))

    def test_root_and_single_hash_argument_are_required_before_reads(self):
        for uid, args in ((1, ["setup", "0" * 64]), (0, ["setup"]), (0, ["setup", "hash", "extra"])):
            with patch.object(setup.os, "geteuid", return_value=uid), patch.object(setup.sys, "argv", args), \
                    patch.object(setup, "read", side_effect=AssertionError("must not read")), \
                    self.assertRaises(ValueError):
                setup.main()

    def test_setup_input_reads_are_bounded_regular_and_nonblocking(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            data = root / "input"
            data.write_bytes(b"12345678")
            self.assertEqual(setup.read(data, 8), b"12345678")
            with self.assertRaises(ValueError):
                setup.read(data, 7)
            alias = root / "alias"
            alias.symlink_to(data)
            with self.assertRaises(OSError):
                setup.read(alias, 8)
            fifo = root / "fifo"
            os.mkfifo(fifo)
            with self.assertRaises(ValueError):
                setup.read(fifo, 8)

    def test_actual_setup_checks_reader_credentials_and_both_probes_before_timer(self):
        for bad_reader, mode, uid, failed in ((False, 0o600, 0, None), (True, 0o600, 0, None),
                (False, 0o640, 0, None), (False, 0o600, 1, None),
                (False, 0o600, 0, "radar-paper-initialize.service"),
                (False, 0o600, 0, "radar-paper-maintenance.service")):
            with self.context(bad_reader, mode, uid, failed) as case:
                if bad_reader or mode != 0o600 or uid != 0:
                    with self.assertRaises(ValueError):
                        setup.main()
                    self.assertEqual(case.calls, [])
                    self.assertFalse(case.library.exists())
                elif failed:
                    with self.assertRaises(setup.subprocess.CalledProcessError):
                        setup.main()
                    self.assertFalse(any(call[1] == "enable" for call in case.calls))
                    if failed == "radar-paper-initialize.service":
                        self.assertFalse(any("radar-paper-maintenance.service" in call for call in case.calls))
                else:
                    setup.main()
                    self.assertEqual(case.calls, [["/usr/bin/systemctl", "daemon-reload"],
                        ["/usr/bin/systemctl", "start", "radar-paper-initialize.service"],
                        ["/usr/bin/systemctl", "start", "radar-paper-maintenance.service"],
                        ["/usr/bin/systemctl", "enable", "--now", "radar-paper-maintenance.timer"]])
                    self.assertEqual((case.library / "radar").read_bytes(), b"verified-reader")
                    case.calls.clear()
                    with self.assertRaises(ValueError):
                        setup.main()
                    self.assertEqual(case.calls, [])
                self.assertNotIn(str(case.credentials), case.inputs)

    def test_prior_library_state_aliases_and_targets_refuse_before_installation(self):
        for kind in ("library", "state", "private-state", "target", "dangling-target"):
            with self.context() as case:
                if kind == "library":
                    case.library.mkdir()
                elif kind in ("state", "private-state"):
                    case.states[kind == "private-state"].mkdir()
                else:
                    target = Path(case.files["radar-paper-maintenance.service"][0])
                    if kind == "target":
                        target.write_text("prior")
                    else:
                        target.symlink_to(case.root / "missing")
                with self.assertRaises(ValueError):
                    setup.main()
                self.assertEqual(case.calls, [])
                self.assertFalse((case.library / "radar").exists())
