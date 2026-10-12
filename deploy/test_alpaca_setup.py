# SPDX-License-Identifier: Apache-2.0
"""No real credentials, network requests or root writes."""

import importlib.util
import json
import os
from pathlib import Path
import stat
import tempfile
import types
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("setup", Path(__file__).with_name("setup-alpaca-paper.py"))
setup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(setup)
KEY = "P" * 20
SECRET = "S" * 40


class Response:
    status = 200

    def __init__(self, raw):
        self.raw = raw

    def read(self, size):
        return self.raw[:size]

    def __enter__(self):
        return self

    def __exit__(self, *args):
        pass


class SetupTests(unittest.TestCase):
    def account(self):
        return dict(id="paper-id", status="ACTIVE", currency="USD",
                    trading_blocked=False, account_blocked=False, trade_suspended_by_user=False)

    def test_fixed_read_and_fail_closed_account(self):
        seen = []

        def read(request, timeout):
            seen.append((request.full_url, request.method, request.headers, timeout))
            return Response(json.dumps(self.account()).encode())

        setup.verify(KEY, SECRET, read)
        self.assertEqual(seen, [("https://paper-api.alpaca.markets/v2/account", "GET",
                               {"Apca-api-key-id": KEY, "Apca-api-secret-key": SECRET}, 10)])
        for field, value in [("id", ""), ("status", "CLOSED"), ("currency", "EUR"),
                             ("trading_blocked", True), ("account_blocked", None),
                             ("trade_suspended_by_user", 0)]:
            account = self.account()
            account[field] = value
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "nothing saved"):
                setup.verify(KEY, SECRET, lambda *a, **k: Response(json.dumps(account).encode()))
        for raw in [b"[1]", b"invalid secret", b" " * 65537]:
            with self.assertRaisesRegex(ValueError, "nothing saved"):
                setup.verify(KEY, SECRET, lambda *a, **k: Response(raw))

    def test_redirect_and_transport_never_echo_secret(self):
        with self.assertRaises(ValueError):
            setup.NoRedirect().redirect_request(None, None, 302, "", {}, "https://elsewhere.invalid")

        def fail(*a, **k):
            raise OSError(SECRET)

        with self.assertRaises(ValueError) as error:
            setup.verify(KEY, SECRET, fail)
        self.assertNotIn(SECRET, str(error.exception))

    def test_private_atomic_save_refuses_overwrite_symlink_and_injection(self):
        real_stat = Path.lstat

        def root_stat(path):
            info = real_stat(path)
            return types.SimpleNamespace(st_mode=info.st_mode & ~0o022, st_uid=0)

        with tempfile.TemporaryDirectory() as directory, patch.object(Path, "lstat", root_stat):
            setup.save(directory, KEY, SECRET)
            target = Path(directory) / "alpaca-paper.env"
            expected = f"RADAR_ALPACA_PAPER_KEY_ID={KEY}\nRADAR_ALPACA_PAPER_SECRET_KEY={SECRET}\n"
            self.assertEqual(target.read_text(), expected)
            self.assertEqual(stat.S_IMODE(os.stat(target).st_mode), 0o600)
            with self.assertRaises(FileExistsError):
                setup.save(directory, KEY, SECRET)
            self.assertEqual(target.read_text(), expected)
            self.assertEqual(sorted(p.name for p in Path(directory).iterdir()), [target.name])
            target.unlink()
            target.symlink_to(Path(directory) / "absent")
            with self.assertRaises(FileExistsError):
                setup.save(directory, KEY, SECRET)
            self.assertFalse((Path(directory) / "absent").exists())
        for value in ["", "short", "x" * 129, KEY + "\nOTHER=value", SECRET + "'", " " + KEY]:
            with self.assertRaises(ValueError):
                setup.credential(value)

    def test_untrusted_directory_refused(self):
        for mode, uid in [(stat.S_IFDIR | 0o777, 0), (stat.S_IFDIR | 0o755, 1),
                          (stat.S_IFLNK | 0o700, 0)]:
            with patch.object(Path, "lstat", return_value=types.SimpleNamespace(st_mode=mode, st_uid=uid)):
                with self.assertRaises(ValueError):
                    setup.save("/unused", KEY, SECRET)
