#!/usr/bin/env python3
"""Offline tests for the database-only generator; never starts Docker."""
import contextlib
import importlib.util
import io
import os
from pathlib import Path
import re
import stat
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
from common import SafetyError

spec = importlib.util.spec_from_file_location("postgres_only", HERE / "postgres-only.py")
generator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generator)


class PostgresOnlyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="deciduous-postgres-generator-")
        self.root = Path(self.temp.name)

    def tearDown(self):
        self.temp.cleanup()

    def test_exact_templates_private_credentials_and_no_secret_output(self):
        target = self.root / "postgres"
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            generator.main(["--output", str(target)])
        self.assertEqual({p.name for p in target.iterdir()}, set(generator.FILES) | {".env"})
        for name in generator.FILES:
            self.assertEqual((target / name).read_bytes(), (generator.TEMPLATES / name).read_bytes())
        env = dict(line.split("=", 1) for line in (target / ".env").read_text().splitlines())
        self.assertEqual(env["POSTGRES_DB"], "deciduous")
        self.assertEqual(env["POSTGRES_USER"], "deciduous")
        self.assertEqual(env["POSTGRES_PORT"], "55432")
        self.assertRegex(env["POSTGRES_PASSWORD"], r"^[a-f0-9]{64}$")
        self.assertNotIn(env["POSTGRES_PASSWORD"], output.getvalue())
        self.assertEqual(stat.S_IMODE((target / ".env").stat().st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(target.stat().st_mode), 0o700)
        self.assertIn("No containers started", output.getvalue())

    def test_custom_port_and_password_uniqueness(self):
        one = generator.generate(self.root / "one", 55433)
        two = generator.generate(self.root / "two", 65432)
        self.assertIn("POSTGRES_PORT=55433\n", (one / ".env").read_text())
        self.assertNotEqual((one / ".env").read_text().splitlines()[2],
                            (two / ".env").read_text().splitlines()[2])

    def test_refuse_existing_directory_file_and_dangling_symlink(self):
        directory = self.root / "existing"
        directory.mkdir()
        file = self.root / "file"
        file.write_text("Keep me")
        link = self.root / "link"
        link.symlink_to(self.root / "missing")
        for target in (directory, file, link):
            with self.subTest(target=target), self.assertRaises(SafetyError):
                generator.generate(target)
        self.assertEqual(file.read_text(), "Keep me")
        self.assertTrue(link.is_symlink())
        self.assertEqual(list(directory.iterdir()), [])

    def test_refuse_symlink_or_missing_parent(self):
        link = self.root / "parent-link"
        link.symlink_to(self.root, target_is_directory=True)
        for target in (link / "child", self.root / "absent" / "child"):
            with self.subTest(target=target), self.assertRaises(SafetyError):
                generator.generate(target)

    def test_invalid_port_creates_nothing(self):
        for port in (0, 80, 1023, 65536):
            target = self.root / str(port)
            with self.assertRaises(SafetyError):
                generator.generate(target, port)
            self.assertFalse(target.exists())

    def test_failed_write_only_cleans_own_files(self):
        target = self.root / "failed"
        real_open = os.open
        def fail(path, flags, mode):
            if Path(path).name == "compose.yaml":
                (target / "unexpected.txt").write_text("Keep unrelated file")
                raise OSError("simulated write failure")
            return real_open(path, flags, mode)
        with patch.object(generator.os, "open", side_effect=fail), self.assertRaises(OSError):
            generator.generate(target)
        self.assertEqual({p.name for p in target.iterdir()}, {"unexpected.txt"})
        self.assertEqual((target / "unexpected.txt").read_text(), "Keep unrelated file")

    def test_template_is_one_loopback_postgres_service(self):
        dockerfile = (generator.TEMPLATES / "Dockerfile").read_text()
        compose = (generator.TEMPLATES / "compose.yaml").read_text()
        self.assertEqual(dockerfile.strip(), "FROM postgres:16-bookworm")
        self.assertIn('"127.0.0.1:${POSTGRES_PORT:-55432}:5432"', compose)
        self.assertIn("postgres-data:/var/lib/postgresql/data", compose)
        self.assertIn("pg_isready", compose)
        self.assertEqual(re.findall(r"^  (\w+):$", compose, flags=re.MULTILINE), ["db"])
        self.assertNotIn("container_name:", compose)


if __name__ == "__main__":
    unittest.main()
