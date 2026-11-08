#!/usr/bin/env python3
"""Read-only verification of one workspace through original and restored test apps."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from common import base_url, get_graph, read_private, request

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--source-url", required=True)
parser.add_argument("--restored-url", required=True)
parser.add_argument("--workspace", required=True)
parser.add_argument("--env-file", type=Path, required=True)
args = parser.parse_args()
urls = [base_url(args.source_url), base_url(args.restored_url)]
if any(not url.startswith("http://127.0.0.1:") for url in urls):
    parser.error("Use isolated loopback test servers")
config = dict(line.split("=", 1) for line in read_private(args.env_file).splitlines() if "=" in line)
token = config["DECIDUOUS_MCP_TOKEN"]
graphs = [get_graph(url, token, args.workspace) for url in urls]
for key in ("nodes", "edges", "documents", "themes", "node_themes"):
    original = sorted(json.dumps(row, sort_keys=True) for row in graphs[0].get(key, []))
    restored = sorted(json.dumps(row, sort_keys=True) for row in graphs[1].get(key, []))
    assert original == restored, f"Restored {key} differ"
for doc in graphs[0].get("documents", []):
    content = [request(url + "/documents/" + doc["id"], token) for url in urls]
    assert hashlib.sha256(content[0]).digest() == hashlib.sha256(content[1]).digest()
print("PASS: restored graph records and attachment checksums match through the actual HTTP server")
