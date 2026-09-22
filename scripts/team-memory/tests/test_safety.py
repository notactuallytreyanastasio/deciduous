#!/usr/bin/env python3
"""Offline regression tests; no Docker daemon, database server, or credentials required."""
import contextlib
import hashlib
import importlib.util
import io
import os
from pathlib import Path
import shutil
import sqlite3
import stat
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
from common import SafetyError, base_url, private_write, read_private, workspace_name


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, HERE / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


stack = load("local_stack", "local-stack.py")
migration = load("migration", "migrate-sqlite.py")


class SafetyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="deciduous-script-test-")
        self.root = Path(self.temp.name)

    def tearDown(self):
        self.temp.cleanup()

    def fixture(self):
        project = self.root / "project"
        directory = project / ".deciduous"
        directory.mkdir(parents=True)
        database = directory / "deciduous.db"
        with sqlite3.connect(database) as conn:
            conn.execute("CREATE TABLE decision_nodes (id INTEGER PRIMARY KEY, title TEXT)")
            conn.execute("INSERT INTO decision_nodes VALUES (1, 'Keep me')")
        (directory / "documents").mkdir()
        (directory / "documents" / "evidence.txt").write_text("keep attachment")
        return project, database

    def test_configure_plan_creates_nothing(self):
        config = self.root / "new" / "local.env"
        with contextlib.redirect_stdout(io.StringIO()):
            stack.main(["configure", "--env-file", str(config)])
        self.assertFalse(config.parent.exists())

    def test_configure_private_idempotent_and_silent_secrets(self):
        config = self.root / "new" / "local.env"
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            stack.main(["configure", "--env-file", str(config), "--apply"])
            before = config.read_bytes()
            stack.main(["configure", "--env-file", str(config), "--apply"])
        self.assertEqual(before, config.read_bytes())
        self.assertEqual(stat.S_IMODE(config.stat().st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(config.parent.stat().st_mode), 0o700)
        for secret in stack.settings(config).values():
            if len(secret) == 64:
                self.assertNotIn(secret, output.getvalue())

    def test_invalid_port_creates_nothing(self):
        with self.assertRaises(SafetyError):
            stack.main(["configure", "--env-file", str(self.root / "bad"), "--port", "80", "--apply"])

    def test_secret_mode_and_symlink_rejected(self):
        secret = self.root / "secret"
        private_write(secret, "secret")
        secret.chmod(0o644)
        with self.assertRaises(SafetyError):
            read_private(secret)
        link = self.root / "link"
        link.symlink_to(secret)
        with self.assertRaises(SafetyError):
            read_private(link)

    def test_no_credential_urls_or_plain_remote_http(self):
        for url in ("http://remote.example", "https://token@example.com", "https://example.com/?token=x",
                    "https://example.com/#secret", "https://example.com/mcp", "file:///tmp/db"):
            with self.subTest(url=url), self.assertRaises(SafetyError):
                base_url(url)
        self.assertEqual(base_url("http://127.0.0.1:4000/"), "http://127.0.0.1:4000")

    def test_workspace_requires_explicit_safe_slug(self):
        for name in ("*", "../project", "Project", "", "a b"):
            with self.assertRaises(SafetyError):
                workspace_name(name)

    def test_migration_plan_does_not_export_or_request(self):
        project, database = self.fixture()
        before = database.read_bytes()
        backup = self.root / "backup"
        with patch.object(migration, "request", side_effect=AssertionError("network")), \
                patch.object(migration, "load_export", side_effect=AssertionError("export")), \
                contextlib.redirect_stdout(io.StringIO()):
            migration.main(["--project", str(project), "--url", "http://127.0.0.1:4000",
                            "--workspace", "fixture", "--backup-dir", str(backup)])
        self.assertFalse(backup.exists())
        self.assertEqual(before, database.read_bytes())

    def test_snapshot_preserves_original_and_attachment(self):
        project, database = self.fixture()
        before = database.read_bytes()
        preserved, exported = migration.snapshot(project, database, self.root / "backup")
        self.assertEqual(before, database.read_bytes())
        self.assertEqual((preserved / "documents" / "evidence.txt").read_text(), "keep attachment")
        with sqlite3.connect(exported / ".deciduous" / "deciduous.db") as conn:
            conn.execute("UPDATE decision_nodes SET title='Copy changed'")
        with sqlite3.connect(preserved / "deciduous.db") as conn:
            self.assertEqual(conn.execute("SELECT title FROM decision_nodes").fetchone()[0], "Keep me")

    def test_wal_without_shm_is_read_without_source_side_effects(self):
        # Hold a WAL writer in another directory; copy only DB+WAL to emulate
        # an archived project missing its ephemeral shared-memory sidecar.
        origin = self.root / "origin.db"
        writer = sqlite3.connect(origin)
        self.addCleanup(writer.close)
        writer.execute("PRAGMA journal_mode=WAL")
        writer.execute("CREATE TABLE decision_nodes (id INTEGER PRIMARY KEY, title TEXT)")
        writer.execute("INSERT INTO decision_nodes VALUES (1, 'Committed WAL data')")
        writer.commit()
        project = self.root / "wal-project"
        directory = project / ".deciduous"
        directory.mkdir(parents=True)
        database = directory / "deciduous.db"
        shutil.copyfile(origin, database)
        shutil.copyfile(str(origin) + "-wal", str(database) + "-wal")
        before = {p.name: p.read_bytes() for p in directory.iterdir()}
        _, counts = migration.inspect(project)
        self.assertEqual(counts["decision_nodes"], 1)
        preserved, _ = migration.snapshot(project, database, self.root / "wal-backup")
        self.assertEqual(before, {p.name: p.read_bytes() for p in directory.iterdir()})
        with sqlite3.connect(preserved / "deciduous.db") as conn:
            self.assertEqual(conn.execute("SELECT title FROM decision_nodes").fetchone()[0], "Committed WAL data")

    def test_backup_cannot_overwrite_or_recurse_into_project(self):
        project, database = self.fixture()
        with self.assertRaises(SafetyError):
            migration.snapshot(project, database, self.root)
        with self.assertRaises(SafetyError):
            migration.snapshot(project, database, project / "backup")

    def test_nonempty_workspace_and_required_bytes_stop(self):
        with self.assertRaises(SafetyError):
            migration.require_empty({"nodes": [{"title": "existing"}]})
        graph = {"nodes": [{"id": 1, "change_id": "node1"}], "edges": [],
                 "documents": [{"node_change_id": "node1", "change_id": "doc1", "content_hash": "a" * 64}]}
        with self.assertRaises(SafetyError):
            migration.validate_graph(graph, self.root / "documents")

    def test_self_loops_and_duplicate_edges_stop(self):
        graph = {"nodes": [{"id": 1, "change_id": "n1"}, {"id": 2, "change_id": "n2"}],
                 "edges": [{"from_node_id": 1, "to_node_id": 1}], "documents": []}
        with self.assertRaises(SafetyError):
            migration.validate_graph(graph, self.root)
        graph["edges"] = [{"from_node_id": 1, "to_node_id": 2}] * 2
        with self.assertRaises(SafetyError):
            migration.validate_graph(graph, self.root)

    def test_attachment_content_addressing(self):
        content = b"the actual bytes"
        path = self.root / "renamed-file.txt"
        path.write_bytes(content)
        digest = hashlib.sha256(content).hexdigest()
        graph = {"nodes": [{"id": 1, "change_id": "node1"}], "edges": [],
                 "documents": [{"node_change_id": "node1", "change_id": "doc1", "content_hash": digest}]}
        edges, blobs = migration.validate_graph(graph, self.root)
        self.assertEqual(edges, set())
        self.assertEqual(blobs, {digest: path})

    def configured(self):
        config = self.root / "local.env"
        with contextlib.redirect_stdout(io.StringIO()):
            stack.main(["configure", "--env-file", str(config), "--apply"])
        return config

    def test_up_backup_upgrade_plans_do_not_run_commands(self):
        config = self.configured()
        for command in ("up", "backup", "upgrade"):
            with self.subTest(command=command), patch.object(stack, "Stack") as mocked, \
                    contextlib.redirect_stdout(io.StringIO()):
                mocked.return_value.url = "http://127.0.0.1:4000"
                stack.main([command, "--env-file", str(config)])
                mocked.return_value.run.assert_not_called()
                mocked.return_value.backup.assert_not_called()

    def test_upgrade_requires_paused_writers(self):
        config = self.configured()
        with patch.object(stack, "Stack") as mocked, self.assertRaises(SafetyError):
            stack.main(["upgrade", "--env-file", str(config), "--apply"])
        self.assertEqual(mocked.return_value.run.call_count, 1)  # config --quiet only

    def test_backup_failure_prevents_app_replacement(self):
        config = self.configured()
        with patch.object(stack, "Stack") as mocked, patch.object(stack.subprocess, "run") as process:
            process.return_value = SimpleNamespace(stdout="sha256:old-image")
            def run(*args, **kwargs):
                if args[0] == "ps":
                    return SimpleNamespace(stdout="fixture-container")
                if args[0] == "exec":
                    return SimpleNamespace(stdout="160014")
                return SimpleNamespace(stdout="")
            mocked.return_value.run.side_effect = run
            mocked.return_value.backup.side_effect = SafetyError("backup failed")
            with self.assertRaisesRegex(SafetyError, "backup failed"):
                stack.main(["upgrade", "--env-file", str(config), "--apply", "--writers-paused"])
            self.assertFalse(any(call.args[0] == "up" for call in mocked.return_value.run.call_args_list))

    def test_major_version_change_is_refused(self):
        config = self.configured()
        with patch.object(stack, "Stack") as mocked:
            mocked.return_value.run.return_value = SimpleNamespace(stdout="180000")
            with self.assertRaisesRegex(SafetyError, "PostgreSQL 16"):
                stack.main(["upgrade", "--env-file", str(config), "--apply", "--writers-paused"])
            mocked.return_value.backup.assert_not_called()
            self.assertFalse(any(call.args[0] in ("up", "build") for call in mocked.return_value.run.call_args_list))


if __name__ == "__main__":
    unittest.main()
