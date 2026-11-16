#!/usr/bin/env python3
"""Test harness only: run the MCP app against an existing isolated local database."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import urllib.parse

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from common import read_private

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--env-file", type=Path, required=True)
parser.add_argument("--database-url", required=True)
parser.add_argument("--port", type=int, required=True)
args = parser.parse_args()
database = urllib.parse.urlsplit(args.database_url)
if database.hostname != "127.0.0.1" or database.port == 5432 or not database.path.endswith("_test"):
    parser.error("Use an isolated loopback cluster on a non-default port and a database ending _test")
config = dict(line.split("=", 1) for line in read_private(args.env_file).splitlines() if "=" in line)
env = os.environ.copy()
env.update(MIX_ENV="prod", DATABASE_URL=args.database_url,
           DECIDUOUS_MCP_TOKEN=config["DECIDUOUS_MCP_TOKEN"], PORT=str(args.port))
source = Path(__file__).resolve().parents[3] / "deciduous_mcp"
subprocess.run(["mix", "ecto.migrate"], cwd=source, env=env, check=True)
os.chdir(source)
os.execvpe("mix", ["mix", "run", "--no-halt"], env)
