#!/usr/bin/env python3
"""Explicit integration test against an isolated server; creates a unique test workspace."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
from common import base_url, read_private


def hashes(directory):
    return {str(p.relative_to(directory)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in directory.rglob("*") if p.is_file()}


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--url", required=True)
parser.add_argument("--cli", type=Path, required=True)
parser.add_argument("--env-file", type=Path, required=True)
args = parser.parse_args()
url = base_url(args.url)
if not url.startswith("http://127.0.0.1:"):
    parser.error("Use an isolated loopback test server")
config = dict(line.split("=", 1) for line in read_private(args.env_file).splitlines() if "=" in line)
env = os.environ.copy()
env["DECIDUOUS_MCP_TOKEN"] = config["DECIDUOUS_MCP_TOKEN"]
root = Path(tempfile.mkdtemp(prefix="deciduous-live-migration-"))
project = root / "project"
(project / ".deciduous" / "documents").mkdir(parents=True)
env["DECIDUOUS_DB_PATH"] = str(project / ".deciduous" / "deciduous.db")
cli = str(args.cli.resolve())


def local(*command, cwd=project, environment=env):
    return subprocess.run([cli, *command], cwd=cwd, env=environment, check=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)


local("add", "goal", "Migration fixture: preserve this goal")
local("add", "action", "Migration fixture: preserve this edge and attachment")
local("link", "1", "2", "-r", "Evidence survives migration")
evidence = project / "evidence.txt"
evidence.write_text("Deciduous migration fixture attachment\n")
local("doc", "attach", "2", str(evidence))
before = hashes(project / ".deciduous")
workspace = "migration-smoke-" + secrets.token_hex(4)
backup = root / "migration-backup"
command = [sys.executable, str(HERE / "migrate-sqlite.py"), "--project", str(project),
           "--url", url, "--workspace", workspace, "--backup-dir", str(backup), "--cli", cli]
subprocess.run(command, env=env, check=True)
assert not backup.exists(), "dry run wrote backup"
assert hashes(project / ".deciduous") == before, "dry run changed source"
apply = ["--apply", "--writers-paused", "--confirm-new-workspace", workspace, "--accept-limitations"]
subprocess.run(command + apply, env=env, check=True)
assert hashes(project / ".deciduous") == before, "migration changed source"
retry = subprocess.run(command + apply, env=env, capture_output=True, text=True)
assert retry.returncode != 0 and "Target workspace contains data" in retry.stderr, "populated target was not refused"

cache = root / "cache"
(cache / ".deciduous").mkdir(parents=True)
cache_env = env.copy()
cache_env["DECIDUOUS_DB_PATH"] = str(cache / ".deciduous" / "deciduous.db")
local("remote", "init", url, "--workspace", workspace, cwd=cache, environment=cache_env)
local("remote", "status", cwd=cache, environment=cache_env)
local("remote", "pull", cwd=cache, environment=cache_env)
graph = json.loads(local("graph", cwd=cache, environment=cache_env).stdout)
assert len(graph["nodes"]) == 2 and len(graph["edges"]) == 1
assert len(graph.get("documents", [])) == 0, "update docs if pull now carries documents"
print("PASS: live import, attachment checksum, source preservation, populated-target refusal, CLI init/status/pull")
print(f"Retained isolated fixtures for inspection: {root}")
