#!/usr/bin/env python3
"""Operate the bundled local Compose stack. Stateful commands default to a plan."""

import argparse
import datetime as dt
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import subprocess
import sys
import time

from common import SafetyError, get_graph, private_directory, private_write, read_private, request

HERE = Path(__file__).resolve().parent
DEFAULT_ENV = Path.home() / ".config" / "deciduous" / "team-memory" / "local.env"
ALLOWED = {"POSTGRES_PASSWORD", "DECIDUOUS_MCP_TOKEN", "MCP_PORT", "MCP_IMAGE_TAG"}


def settings(path):
    result = {}
    for line in read_private(path).splitlines():
        if not line or line.startswith("#"):
            continue
        key, separator, value = line.partition("=")
        if not separator or key not in ALLOWED or key in result:
            raise SafetyError("Unexpected or duplicate setting in the env file")
        result[key] = value
    for key in ("POSTGRES_PASSWORD", "DECIDUOUS_MCP_TOKEN"):
        if not re.fullmatch(r"[a-f0-9]{64}", result.get(key, "")):
            raise SafetyError(f"{key} must be 64 hex characters in the local env file")
    if not result.get("MCP_PORT", "").isdigit() or not 1024 <= int(result["MCP_PORT"]) <= 65535:
        raise SafetyError("MCP_PORT must be between 1024 and 65535")
    if not re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9_.-]{0,127}", result.get("MCP_IMAGE_TAG", "")):
        raise SafetyError("Invalid MCP_IMAGE_TAG")
    return result


class Stack:
    def __init__(self, args, config):
        self.args = args
        self.config = config
        self.env = os.environ.copy()
        # Explicit values win over arbitrary exported variables on this machine.
        self.env.update(config)
        compose = ["docker", "compose"]
        if shutil.which("docker-compose") and subprocess.run(
                ["docker", "compose", "version"], stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL).returncode:
            compose = ["docker-compose"]
        self.prefix = compose + ["--project-name", args.project,
                       "--env-file", str(args.env_file), "--file", str(HERE / "compose.yaml")]
        self.url = "http://127.0.0.1:" + config["MCP_PORT"]

    def run(self, *args, **kwargs):
        return subprocess.run(self.prefix + list(args), env=self.env, check=True, **kwargs)

    def check(self):
        self.run("config", "--quiet", stdout=subprocess.DEVNULL)
        self.run("exec", "-T", "db", "psql", "-U", "deciduous", "-d", "deciduous",
                 "-v", "ON_ERROR_STOP=1", "-Atc", "SELECT 1", stdout=subprocess.DEVNULL, timeout=15)
        request(self.url + "/health", timeout=15)
        get_graph(self.url, self.config["DECIDUOUS_MCP_TOKEN"], "setup-check", timeout=15)
        print("PASS: Compose config, Postgres query, HTTP health, authenticated graph export")

    def wait(self):
        deadline = time.monotonic() + 120
        while time.monotonic() < deadline:
            try:
                request(self.url + "/health", timeout=5)
                self.check()
                return
            except (SafetyError, subprocess.SubprocessError):
                time.sleep(2)
        raise SafetyError("Server not ready after 120 seconds; inspect Compose logs")

    def backup(self):
        directory = private_directory(self.args.backup_dir)
        name = "deciduous-" + dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S")
        name += "-" + secrets.token_hex(3) + ".dump"
        target = directory / name
        with os.fdopen(os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "wb") as out:
            self.run("exec", "-T", "db", "pg_dump", "-U", "deciduous", "-d", "deciduous",
                     "--format=custom", stdout=out)
        if not target.stat().st_size:
            raise SafetyError(f"Empty backup retained for inspection: {target}")
        with target.open("rb") as source:
            self.run("exec", "-T", "db", "pg_restore", "--list", stdin=source,
                     stdout=subprocess.DEVNULL)
        print(f"Backup archive created and readable: {target}")
        print("A readable archive is not a restore test. Restore it into an isolated database.")
        return target


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["configure", "up", "check", "token", "backup", "upgrade"])
    parser.add_argument("--apply", action="store_true", help="perform this stateful operation")
    parser.add_argument("--env-file", type=Path, default=DEFAULT_ENV)
    parser.add_argument("--project", default="deciduous-team-memory")
    parser.add_argument("--port", type=int, default=4000, help="configure only; loopback port")
    parser.add_argument("--backup-dir", type=Path, default=DEFAULT_ENV.parent / "backups")
    parser.add_argument("--writers-paused", action="store_true", help="confirm agents are stopped for upgrade")
    args = parser.parse_args(argv)
    if not re.fullmatch(r"[a-z0-9][a-z0-9_-]{0,62}", args.project):
        raise SafetyError("Use a lowercase Compose project name")
    args.env_file = args.env_file.expanduser().absolute()
    if args.command == "configure":
        if not 1024 <= args.port <= 65535:
            raise SafetyError("Port must be between 1024 and 65535")
        if args.env_file.exists():
            settings(args.env_file)
            print("Existing private config is valid; nothing changed")
            return
        print(f"Create private credentials at {args.env_file}; app port 127.0.0.1:{args.port}")
        if not args.apply:
            print("Plan only. Re-run with --apply; no files or containers changed.")
            return
        private_directory(args.env_file.parent)
        private_write(args.env_file, "POSTGRES_PASSWORD=" + secrets.token_hex(32)
                      + "\nDECIDUOUS_MCP_TOKEN=" + secrets.token_hex(32)
                      + f"\nMCP_PORT={args.port}\nMCP_IMAGE_TAG=local\n")
        print("Configured. Credentials were not printed; existing files are never overwritten.")
        return
    config = settings(args.env_file)
    if args.command == "token":
        if sys.stdout.isatty():
            raise SafetyError("Token output requires a pipe or command substitution, not a terminal")
        print(config["DECIDUOUS_MCP_TOKEN"])
        return
    stack = Stack(args, config)
    if args.command == "check":
        stack.check()
        return
    if not args.apply:
        print(f"Plan: {args.command} Compose project {args.project}, app at {stack.url}")
        if args.command in ("backup", "upgrade"):
            print(f"Create a private pg_dump archive in {args.backup_dir}")
        if args.command == "upgrade":
            print("Build the app, back up Postgres, replace only mcp; leave db and volume unchanged")
        print("Plan only. Re-run with --apply; no files, images, containers, or database changed.")
        return
    stack.run("config", "--quiet")
    if args.command == "up":
        stack.run("up", "-d", "--build")
        stack.wait()
    elif args.command == "backup":
        stack.backup()
    elif args.command == "upgrade":
        if not args.writers_paused:
            raise SafetyError("Pause agents, then confirm with --writers-paused")
        running = stack.run("ps", "--status", "running", "--quiet", "mcp", capture_output=True, text=True)
        if not running.stdout.strip():
            raise SafetyError("Expected an existing running mcp service; use up for a new installation")
        major = stack.run("exec", "-T", "db", "psql", "-U", "deciduous", "-d", "deciduous",
                          "-Atc", "SHOW server_version_num", capture_output=True, text=True)
        if not major.stdout.strip().isdigit() or int(major.stdout.strip()) // 10000 != 16:
            raise SafetyError("This stack requires PostgreSQL 16; major upgrades need a separate plan")
        old_image = subprocess.run(["docker", "inspect", "--format", "{{.Image}}", running.stdout.strip()],
                                   check=True, capture_output=True, text=True).stdout.strip()
        stack.run("build", "mcp")
        archive = stack.backup()
        private_write(str(archive) + ".json", json.dumps({"previous_app_image": old_image,
                      "compose_project": args.project, "schema_rollback": "manual restore into a new database"}, indent=2))
        stack.run("up", "-d", "--no-deps", "--no-build", "mcp")
        stack.wait()
        print("App upgrade complete. Keep the backup and old image until the team verifies its graph.")


if __name__ == "__main__":
    try:
        main()
    except (SafetyError, OSError, subprocess.SubprocessError) as error:
        # CalledProcessError argv never contains secrets in this script.
        print(f"Stopped: {error}", file=sys.stderr)
        sys.exit(1)
