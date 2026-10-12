# SPDX-License-Identifier: Apache-2.0
"""Temporary captures and fake providers; no live credentials or service changes."""

import fcntl
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


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


worker = load("worker", "record-alpaca-market.py")
setup = load("setup", "setup-alpaca-recorder.py")


class RecorderTests(unittest.TestCase):
    def producer(self, calls, metadata=None, capture=None):
        def run(args, **kwargs):
            calls.append((args, kwargs))
            body = dict(format="alpaca-iex-snapshot-v1", read_started_unix_ms=99,
                        available_at_unix_ms=100, symbols=["SPY", "QQQ"],
                        raw_response=json.dumps({"SPY": {"latestQuote": None}, "QQQ": None}))
            body.update(capture or {})
            Path(args[-1]).write_text(json.dumps(body))
            report = dict(capture_written=True, feed="iex", execution_authority=False,
                          available_at_unix_ms=100)
            report.update(metadata or {})
            return types.SimpleNamespace(stdout=json.dumps(report))
        return run

    def test_capture_retains_history_and_labels_unknowns(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            prior = directory / "prior.json"
            prior.write_text("history")
            calls = []
            report = worker.observe(directory, self.producer(calls))
            self.assertEqual(prior.read_text(), "history")
            self.assertEqual(report["recording"], "captured")
            self.assertEqual(report["unknown_symbols"], ["QQQ"])
            self.assertEqual(report["unknown_quote_symbols"], ["SPY", "QQQ"])
            self.assertFalse(report["execution_authority"])
            self.assertFalse(report["exchange_times_validated"])
            self.assertFalse(report["consolidated_market"])
            self.assertEqual(report["available_at_unix_ms"], 100)
            self.assertTrue((directory / report["capture"]).is_file())
            self.assertEqual(calls[0][0][:-1], [worker.READER, "alpaca-market-read", "--symbols", "SPY,QQQ", "--output"])
            self.assertEqual(calls[0][1], dict(check=True, capture_output=True, text=True, timeout=20))

    def test_budget_boundary_and_file_limit_prevent_provider_call(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            (directory / "prior").write_bytes(b"data")
            for limit, files in [(worker.MAX_CAPTURE + 3, 10), (worker.MAX_BYTES, 1)]:
                calls = []
                report = worker.observe(directory, self.producer(calls), max_bytes=limit, max_files=files)
                self.assertEqual(report["recording"], "paused")
                self.assertEqual(report["reason"], "storage_limit")
                self.assertEqual(report["bytes"], 4)
                self.assertEqual(report["files"], 1)
                self.assertEqual(calls, [])
            self.assertEqual(worker.observe(directory, self.producer([]),
                                            max_bytes=worker.MAX_CAPTURE + 4)["recording"], "captured")

    def test_bad_metadata_or_capture_never_reports_success(self):
        for metadata, capture in [({"feed": "sip"}, {}), ({"capture_written": False}, {}),
                                  ({"execution_authority": True}, {}), ({"available_at_unix_ms": True}, {}),
                                  ({"available_at_unix_ms": 100.0}, {}),
                                  ({}, {"available_at_unix_ms": 101}), ({}, {"symbols": ["BTC"]}),
                                  ({}, {"read_started_unix_ms": 100}), ({}, {"format": "other"}),
                                  ({}, {"raw_response": "[]"}), ({}, {"raw_response": '{"SPY":"bad"}'})]:
            with tempfile.TemporaryDirectory() as temporary, self.assertRaises(ValueError):
                worker.observe(Path(temporary), self.producer([], metadata, capture))

    def test_lock_and_symlink_storage_refuse_before_effect(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            calls = []
            with (directory / ".lock").open("wb") as lock:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                with self.assertRaises(BlockingIOError):
                    worker.observe(directory, self.producer(calls))
            (directory / "unsafe").symlink_to(directory / "outside")
            with self.assertRaises(ValueError):
                worker.observe(directory, self.producer(calls))
            self.assertEqual(calls, [])

    def archive(self, names=None, link=False):
        stream = io.BytesIO()
        with tarfile.open(fileobj=stream, mode="w") as archive:
            for name in (list(setup.FILES) if names is None else names):
                entry = tarfile.TarInfo(name)
                body = b"verified"
                entry.size = len(body)
                if link:
                    entry.type = tarfile.SYMTYPE
                    entry.linkname = "/etc/passwd"
                archive.addfile(entry, io.BytesIO(body))
        return stream.getvalue()

    def test_bundle_binds_exact_members_and_bytes(self):
        raw = self.archive()
        digest = hashlib.sha256(raw).hexdigest()
        self.assertEqual(setup.bundle(raw, digest), {name: b"verified" for name in setup.FILES})
        with self.assertRaises(ValueError):
            setup.bundle(raw, "0" * 64)
        for raw in [self.archive([]), self.archive(["../escape"]),
                    self.archive(list(setup.FILES) + [next(iter(setup.FILES))]), self.archive(link=True)]:
            with self.assertRaises(ValueError):
                setup.bundle(raw, hashlib.sha256(raw).hexdigest())

    def test_install_never_overwrites_and_unsafe_directories_refuse(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "worker"
            setup.install(path, b"original", 0o755)
            self.assertEqual(path.read_bytes(), b"original")
            self.assertEqual(stat.S_IMODE(os.stat(path).st_mode), 0o755)
            with self.assertRaises(FileExistsError):
                setup.install(path, b"replacement", 0o755)
            self.assertEqual(path.read_bytes(), b"original")
            self.assertEqual([p.name for p in path.parent.iterdir()], ["worker"])
        for mode, uid in [(stat.S_IFDIR | 0o777, 0), (stat.S_IFDIR | 0o755, 1), (stat.S_IFLNK | 0o755, 0)]:
            with patch.object(Path, "lstat", return_value=types.SimpleNamespace(st_mode=mode, st_uid=uid)):
                with self.assertRaises(ValueError):
                    setup.protected(Path("/unsafe"))

    def test_actual_installer_starts_probe_before_timer_and_refuses_bad_credentials(self):
        for fail_probe, credentials_mode in [(False, 0o600), (True, 0o600), (False, 0o640)]:
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                reader = root / "reader"
                reader.write_bytes(b"verified-reader")
                os.chmod(reader, 0o755)
                credentials = root / "credentials"
                credentials.write_text("private-fixture")
                os.chmod(credentials, credentials_mode)
                archive = root / "bundle"
                raw = self.archive()
                archive.write_bytes(raw)
                files = {name: (str(root / name), mode) for name, (_, mode) in setup.FILES.items()}
                real_open = os.open
                real_stat = Path.lstat
                calls = []

                def path(value):
                    return {"/usr/local/lib/radar-paper/radar": reader,
                            "/etc/radar/alpaca-paper.env": credentials}.get(str(value), Path(value))

                def open_file(value, *args, **kwargs):
                    return real_open(archive if str(value) == "/tmp/radar-paper-market-bundle.tar" else value,
                                     *args, **kwargs)

                def root_stat(value):
                    info = real_stat(value)
                    return types.SimpleNamespace(st_mode=info.st_mode, st_uid=0)

                def run(args, **kwargs):
                    self.assertEqual(kwargs, dict(check=True, env={"PATH": "/usr/bin:/bin"}))
                    calls.append(args)
                    if fail_probe and args[1] == "start":
                        raise setup.subprocess.CalledProcessError(1, args)

                with patch.object(setup.sys, "argv", ["setup.py", hashlib.sha256(raw).hexdigest()]), \
                        patch.object(setup.os, "geteuid", return_value=0), \
                        patch.object(setup.os, "open", side_effect=open_file), \
                        patch.object(Path, "lstat", root_stat), patch.object(setup, "Path", side_effect=path), \
                        patch.object(setup, "FILES", files), patch.object(setup, "protected"), \
                        patch.object(setup, "READER_SHA256", hashlib.sha256(b"verified-reader").hexdigest()), \
                        patch.object(setup.subprocess, "run", side_effect=run), patch("builtins.print"):
                    if credentials_mode != 0o600:
                        with self.assertRaises(ValueError):
                            setup.main()
                        self.assertEqual(calls, [])
                        self.assertFalse((root / "record-alpaca-market.py").exists())
                    elif fail_probe:
                        with self.assertRaises(setup.subprocess.CalledProcessError):
                            setup.main()
                        self.assertEqual(len(calls), 2)
                    else:
                        setup.main()
                        self.assertEqual(calls, [["/usr/bin/systemctl", "daemon-reload"],
                                                ["/usr/bin/systemctl", "start", "radar-paper-market.service"],
                                                ["/usr/bin/systemctl", "enable", "--now", "radar-paper-market.timer"]])
