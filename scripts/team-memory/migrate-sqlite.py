#!/usr/bin/env python3
"""Snapshot one SQLite project and import it into a NEW shared workspace.

Default is an offline, read-only preflight. --apply requires an explicit new
workspace assertion and paused writers. Does not configure clients or remove
local data. Uses the CLI export on a COPY, never on the source SQLite database.
"""

import argparse
import hashlib
import json
import mimetypes
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import urllib.parse

from common import (SafetyError, base_url, get_graph, private_directory,
                    private_write, request, token_from_environment, workspace_name)

MAX_BODY = 64 * 1024 * 1024


def source_db(path):
    return sqlite3.connect(path.as_uri() + "?mode=ro", uri=True)


def file_hash(path):
    hasher = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def copy_database(database, destination):
    """Copy stable DB/WAL bytes without opening SQLite on the source.

    SQLite mode=ro can still create a missing -shm alongside a WAL. Opening
    only a private copy avoids that side effect and still replays committed
    WAL pages (unlike immutable=1). Writers must be paused; compare before,
    copied, and after hashes so an observed change stops the operation.
    """
    candidates = [database, Path(str(database) + "-wal")]
    present = [path for path in candidates if path.exists()]
    before = {path: file_hash(path) for path in present}
    for path in present:
        shutil.copyfile(path, destination / path.name)
    if [path for path in candidates if path.exists()] != present:
        raise SafetyError("SQLite/WAL changed during snapshot; pause writers and retry")
    for path, digest in before.items():
        if file_hash(path) != digest or file_hash(destination / path.name) != digest:
            raise SafetyError("SQLite/WAL changed during snapshot; pause writers and retry")
    return destination / database.name


def inspect(project):
    directory = project / ".deciduous"
    database = directory / "deciduous.db"
    if not database.is_file() or database.is_symlink() or directory.is_symlink():
        raise SafetyError("Project must contain a real .deciduous/deciduous.db")
    # Copying symlinks could include files outside the intended project.
    if any(path.is_symlink() for path in directory.rglob("*")):
        raise SafetyError("Resolve symlinks inside .deciduous before taking a migration snapshot")
    with tempfile.TemporaryDirectory(prefix="deciduous-read-only-preflight-") as scratch:
        copied = copy_database(database, Path(scratch))
        with source_db(copied) as conn:
            tables = {row[0] for row in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")}
            counts = {}
            for name in ("decision_nodes", "decision_edges", "node_documents", "themes", "node_themes"):
                if name in tables:
                    counts[name] = conn.execute(f'SELECT count(*) FROM "{name}"').fetchone()[0]
            if "decision_nodes" not in tables:
                raise SafetyError("Not a supported Deciduous SQLite database")
            if conn.execute("PRAGMA quick_check").fetchone()[0] != "ok":
                raise SafetyError("SQLite quick_check failed; preserve and repair the source before migrating")
    return database, counts


def snapshot(project, database, backup):
    if backup.exists():
        raise SafetyError("Backup path already exists; use a new path for each migration")
    if backup == project or project in backup.parents:
        raise SafetyError("Keep migration backups outside the source project")
    private_directory(backup)
    preserved = backup / "original" / ".deciduous"
    source = project / ".deciduous"
    sqlite_files = {"deciduous.db", "deciduous.db-wal", "deciduous.db-shm"}
    shutil.copytree(source, preserved,
                    ignore=lambda directory, names: sqlite_files if Path(directory) == source else set())
    # Replay committed WAL from a verified stable copy, then compact it with
    # the backup API. No SQLite connection ever opens the source database.
    with tempfile.TemporaryDirectory(prefix="deciduous-snapshot-") as scratch:
        copied = copy_database(database, Path(scratch))
        with source_db(copied) as src, sqlite3.connect(preserved / "deciduous.db") as dest:
            src.backup(dest)
    export_dir = backup / "export-copy"
    shutil.copytree(backup / "original", export_dir)
    return preserved, export_dir


def load_export(cli, export_dir):
    env = os.environ.copy()
    env["DECIDUOUS_DB_PATH"] = str(export_dir / ".deciduous" / "deciduous.db")
    result = subprocess.run([cli, "graph"], cwd=export_dir, env=env, check=True,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=300)
    if len(result.stdout) > MAX_BODY:
        raise SafetyError("Graph is larger than the server import limit (64 MiB)")
    try:
        graph = json.loads(result.stdout)
    except ValueError:
        raise SafetyError("CLI export is not JSON; inspect the retained snapshot with a compatible CLI") from None
    if not isinstance(graph, dict) or not isinstance(graph.get("nodes"), list):
        raise SafetyError("CLI did not export a graph")
    return graph


def validate_graph(graph, documents):
    nodes = graph["nodes"]
    if not nodes:
        raise SafetyError("Source graph is empty; nothing to migrate")
    change_ids = [node.get("change_id") for node in nodes]
    if any(not isinstance(cid, str) or not cid for cid in change_ids) or len(change_ids) != len(set(change_ids)):
        raise SafetyError("Every node needs a unique change_id; upgrade a COPY with a compatible CLI")
    change_ids = set(change_ids)
    # Match the server's integer-ID-first endpoint resolution.
    by_id = {node.get("id"): node["change_id"] for node in nodes}
    edges = set()
    for edge in graph.get("edges", []):
        start = by_id.get(edge.get("from_node_id")) or edge.get("from_change_id")
        end = by_id.get(edge.get("to_node_id")) or edge.get("to_change_id")
        if start not in change_ids or end not in change_ids or start == end:
            raise SafetyError("Source has a self-loop or unresolved edge; inspect a COPY before migration")
        signature = (start, end, edge.get("edge_type") or "leads_to")
        if signature in edges:
            raise SafetyError("Duplicate edge signatures would be collapsed by import; inspect the snapshot")
        edges.add(signature)
    required = {doc.get("content_hash") for doc in graph.get("documents", [])}
    if None in required or any(not isinstance(h, str) or len(h) != 64 for h in required):
        raise SafetyError("A document is missing its content hash")
    document_ids = [doc.get("change_id") for doc in graph.get("documents", [])]
    if len(document_ids) != len(set(document_ids)):
        raise SafetyError("Duplicate document change_id values would make the import ambiguous")
    for doc in graph.get("documents", []):
        if doc.get("node_change_id") not in change_ids or not doc.get("change_id"):
            raise SafetyError("A document has an unresolved node or missing change_id")
    blobs = {}
    if documents.is_dir():
        for path in documents.rglob("*"):
            if path.is_file():
                digest = file_hash(path)
                if digest in required:
                    if path.stat().st_size > MAX_BODY:
                        raise SafetyError("A required attachment exceeds the 64 MiB upload limit")
                    blobs[digest] = path
    if required - blobs.keys():
        raise SafetyError(f"Missing bytes for {len(required - blobs.keys())} document hash(es); recover attachments before import")
    return edges, blobs


def require_empty(graph):
    if any(graph.get(name) for name in ("nodes", "edges", "documents", "themes", "node_themes")):
        raise SafetyError("Target workspace contains data. This tool only seeds a NEW workspace; nothing imported")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", type=Path, required=True)
    parser.add_argument("--url", required=True, help="server base URL, not /mcp")
    parser.add_argument("--workspace", required=True)
    parser.add_argument("--backup-dir", type=Path, required=True, help="new directory outside the source project")
    parser.add_argument("--cli", default="deciduous", help="compatible CLI binary used only on a copy")
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--writers-paused", action="store_true")
    parser.add_argument("--confirm-new-workspace", help="repeat workspace name; assert it has never held data")
    parser.add_argument("--accept-limitations", action="store_true", help="acknowledge themes/history omissions described in upgrading guide")
    args = parser.parse_args(argv)
    project = args.project.expanduser().resolve()
    backup = args.backup_dir.expanduser().resolve()
    url = base_url(args.url)
    workspace = workspace_name(args.workspace)
    database, counts = inspect(project)
    print("Source counts: " + ", ".join(f"{key}={value}" for key, value in counts.items()))
    print(f"Plan: snapshot {project / '.deciduous'} into {backup}; seed workspace {workspace}")
    print("Imports graph nodes, edges, document metadata and matching bytes. Themes and local logs stay in backup.")
    if not args.apply:
        print("Read-only preflight complete. No project files, config, retained backup, or remote state changed.")
        return
    if not args.writers_paused or args.confirm_new_workspace != workspace or not args.accept_limitations:
        raise SafetyError("Apply requires --writers-paused, --confirm-new-workspace NAME, and --accept-limitations")
    cli = shutil.which(args.cli)
    if not cli:
        raise SafetyError("CLI not found; install/build a compatible Deciduous CLI")
    token = token_from_environment()
    request(url + "/health")
    require_empty(get_graph(url, token, workspace))
    preserved, export_dir = snapshot(project, database, backup)
    graph = load_export(cli, export_dir)
    edges, blobs = validate_graph(graph, preserved / "documents")
    payload = json.dumps({"workspace": workspace, "graph": graph}).encode()
    if len(payload) > MAX_BODY:
        raise SafetyError("Import payload exceeds the 64 MiB server limit")
    private_write(backup / "graph-export.json", json.dumps(graph, indent=2))
    # Check once more after snapshotting. The operator must keep this target
    # exclusive: there is no server-side compare-and-create import transaction.
    require_empty(get_graph(url, token, workspace))
    for digest, path in blobs.items():
        request(url + "/blob/" + digest, token, path.read_bytes(), "PUT",
                mimetypes.guess_type(path.name)[0] or "application/octet-stream")
    report = json.loads(request(url + "/import", token, payload, "POST"))
    private_write(backup / "import-report.json", json.dumps(report, indent=2))
    doc_report = report.get("documents") or {}
    if (report.get("nodes", {}).get("upserted") != len(graph["nodes"])
            or report.get("edges", {}).get("upserted") != len(edges)
            or report.get("edges", {}).get("unresolved", 0)
            or doc_report.get("upserted", 0) != len(graph.get("documents", []))
            or doc_report.get("content_missing", 0) or doc_report.get("orphaned", 0)):
        raise SafetyError("Import reported omissions. Remote may contain imported records; inspect the saved report, do not retry blindly")
    remote = get_graph(url, token, workspace)
    remote_nodes = {node["change_id"]: node for node in remote["nodes"]}
    if set(remote_nodes) != {node["change_id"] for node in graph["nodes"]}:
        raise SafetyError("Remote node identities differ after import; preserve the backup and investigate")
    for node in graph["nodes"]:
        if any(remote_nodes[node["change_id"]].get(key) != node.get(key)
               for key in ("title", "node_type", "status", "description")):
            raise SafetyError("Remote node content differs after import; inspect the retained snapshot")
    remote_edges = {(edge.get("from_change_id"), edge.get("to_change_id"), edge.get("edge_type"))
                    for edge in remote.get("edges", [])}
    if remote_edges != edges:
        raise SafetyError("Remote edge identities differ after import; inspect the retained snapshot")
    if {doc.get("change_id") for doc in remote.get("documents", [])} != {doc["change_id"] for doc in graph.get("documents", [])}:
        raise SafetyError("Remote document identities differ after import; inspect the retained snapshot")
    # Verify bytes through the same authenticated endpoint clients use.
    for digest in blobs:
        data = request(url + "/documents/" + urllib.parse.quote(digest), token)
        if hashlib.sha256(data).hexdigest() != digest:
            raise SafetyError("Remote attachment checksum mismatch; preserve local bytes")
    print(f"PASS: {len(remote_nodes)} node identities/content, {len(edges)} edges, {len(blobs)} attachment checksums")
    print(f"Source preserved. Snapshot and import report: {backup}")
    print("Client configuration is unchanged. Configure the verified workspace using the upgrading guide.")


if __name__ == "__main__":
    try:
        main()
    except (SafetyError, OSError, sqlite3.Error, subprocess.SubprocessError, ValueError) as error:
        print(f"Stopped: {error}", file=sys.stderr)
        sys.exit(1)
