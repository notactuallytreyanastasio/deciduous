"""Exercise a running release over HTTP; standard-library Python only.

Run in the Python client container created by scripts/verify-release.sh.
Fixtures are synthetic. No production credentials or data are used.
"""

import argparse
import base64
import concurrent.futures
import hashlib
import json
import os
import socket
import struct
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from pathlib import Path


BASE = os.environ.get("SERVER_URL", "http://server:4000")
TOKEN = os.environ["DECIDUOUS_MCP_TOKEN"]
CONTENT = b"Existing release document\nUnicode: \xe2\x9c\x93\n\x00binary-safe\xff"
DIGEST = hashlib.sha256(CONTENT).hexdigest()


def request(path, method="GET", body=None, *, token=TOKEN, headers=None, timeout=10):
    supplied = {"Accept": "application/json, text/event-stream"}
    if token is not None:
        supplied["Authorization"] = "Bearer " + token
    if isinstance(body, dict):
        body = json.dumps(body).encode()
        supplied["Content-Type"] = "application/json"
    supplied.update(headers or {})
    req = urllib.request.Request(BASE + path, data=body, headers=supplied, method=method)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as response:
            return response.status, response.headers, response.read()
    except urllib.error.HTTPError as error:
        return error.code, error.headers, error.read()


def decoded(body):
    # Streamable HTTP can respond with either JSON or a finite SSE response.
    if body.startswith(b"event:") or body.startswith(b"data:"):
        for line in body.splitlines():
            if line.startswith(b"data:"):
                return json.loads(line[5:].strip())
    return json.loads(body)


def wait(path="/ready", expected=200):
    deadline = time.monotonic() + 90
    last = "no response"
    # A single 503 during startup proves nothing about TLS rejection. Negative
    # probes must remain unavailable after the connection pool has had time
    # to connect, and must never report ready within that observation window.
    stable_seconds = 10 if expected == 503 else 0
    expected_since = None
    while time.monotonic() < deadline:
        try:
            status, _, body = request(path, token=None, timeout=2)
            if status == expected:
                expected_since = expected_since or time.monotonic()
                if time.monotonic() - expected_since >= stable_seconds:
                    if stable_seconds:
                        print(f"PASS {path}: remained HTTP {expected} for {stable_seconds}s", flush=True)
                    return
            else:
                expected_since = None
                if expected == 503 and status == 200:
                    raise AssertionError(f"{BASE}{path} reported ready when it must be unavailable")
            last = f"HTTP {status}: {body[:200]!r}"
        except (OSError, urllib.error.URLError) as error:
            expected_since = None
            last = str(error)
        time.sleep(0.5)
    raise AssertionError(f"{BASE}{path} never returned {expected}: {last}")


class MCP:
    def __init__(self, workspace=None):
        self.headers = {"MCP-Protocol-Version": "2025-03-26"}
        if workspace:
            self.headers["X-Deciduous-Workspace"] = workspace
        self.counter = 0
        status, headers, body = self.rpc("initialize", {
            "protocolVersion": "2025-03-26", "capabilities": {},
            "clientInfo": {"name": "release-acceptance", "version": "1"},
        })
        assert status == 200 and "result" in decoded(body), (status, body)
        self.session = headers["mcp-session-id"]
        self.headers["Mcp-Session-Id"] = self.session
        status, _, body = request("/mcp", "POST", {
            "jsonrpc": "2.0", "method": "notifications/initialized",
        }, headers=self.headers)
        assert status in (200, 202, 204), (status, body)
        status, _, body = self.rpc("tools/list")
        assert status == 200, (status, body)
        names = {tool["name"] for tool in decoded(body)["result"]["tools"]}
        assert {"add_node", "add_edge", "query_nodes", "get_graph", "check_activity"} <= names

    def rpc(self, method, params=None):
        self.counter += 1
        return request("/mcp", "POST", {
            "jsonrpc": "2.0", "id": self.counter, "method": method,
            "params": params or {},
        }, headers=self.headers)

    def tool(self, name, arguments):
        status, _, body = self.rpc("tools/call", {"name": name, "arguments": arguments})
        assert status == 200, (name, status, body)
        response = decoded(body)
        assert "error" not in response, (name, response)
        result = response["result"]
        assert not result.get("isError"), (name, result)
        return json.loads(next(item["text"] for item in result["content"] if item["type"] == "text"))


class Events:
    """Small websocket reader: verify the real PostgreSQL NOTIFY delivery path."""

    def __init__(self, workspace):
        endpoint = urllib.parse.urlparse(BASE)
        self.sock = socket.create_connection((endpoint.hostname, endpoint.port or 80), timeout=10)
        self.stream = self.sock.makefile("rb")
        key = base64.b64encode(os.urandom(16)).decode()
        path = "/events?" + urllib.parse.urlencode({"workspace": workspace})
        headers = (
            f"GET {path} HTTP/1.1\r\nHost: {endpoint.netloc}\r\n"
            "Upgrade: websocket\r\nConnection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n"
            f"Authorization: Bearer {TOKEN}\r\n\r\n"
        )
        self.sock.sendall(headers.encode())
        assert b" 101 " in self.stream.readline(), "websocket upgrade failed"
        response_headers = {}
        while True:
            line = self.stream.readline()
            if line == b"\r\n":
                break
            assert line, "websocket handshake was closed"
            name, value = line.decode().split(":", 1)
            response_headers[name.lower()] = value.strip()
        accept = base64.b64encode(hashlib.sha1((key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()).digest()).decode()
        assert response_headers["sec-websocket-accept"] == accept

    def read(self):
        prefix = self.stream.read(2)
        assert len(prefix) == 2, "event socket closed"
        opcode, size = prefix[0] & 15, prefix[1] & 127
        if size == 126:
            size = struct.unpack("!H", self.stream.read(2))[0]
        elif size == 127:
            size = struct.unpack("!Q", self.stream.read(8))[0]
        payload = self.stream.read(size)
        assert len(payload) == size
        assert opcode == 1, (opcode, payload)
        return json.loads(payload)

    def close(self):
        self.stream.close()
        self.sock.close()


def export(workspace):
    status, _, body = request("/export?" + urllib.parse.urlencode({"workspace": workspace}))
    assert status == 200, (status, body)
    graph = json.loads(body)
    graph["metadata"].pop("exported_at", None)
    # 1.0.1 exposes branch at the top level as well as inside metadata. Keep
    # the comparison strict while normalizing this additive 1.0.0 API change.
    for node in graph["nodes"]:
        node.setdefault("branch", (node.get("metadata") or {}).get("branch"))
    for key in ("nodes", "edges", "documents", "themes", "node_themes"):
        graph[key] = sorted(graph[key], key=lambda item: json.dumps(item, sort_keys=True))
    return graph


def seed(case):
    workspace = "existing-" + case
    nodes = []
    for index in range(300):
        nodes.append({
            "id": index + 1, "change_id": str(uuid.uuid5(uuid.NAMESPACE_URL, f"{case}/node/{index}")),
            "node_type": "goal" if index == 0 else "observation",
            "title": ("Decision history — café ✓ " * 30) if index == 0 else f"Historical observation {index}",
            "description": f"Preserve the reasoning and Unicode for node {index}: 日本語",
            "status": "completed" if index % 3 == 0 else "active",
            "metadata_json": json.dumps({"branch": "main" if index % 2 else "feature/existing", "confidence": 91, "prompt": "Keep our history"}),
            "created_at": "2025-04-03T11:22:33-04:00", "updated_at": "2025-04-04T12:23:34Z",
        })
    edges = [{"id": i, "from_node_id": i, "to_node_id": i + 1, "edge_type": "leads_to", "rationale": f"Because {i}"} for i in range(1, 300)]
    doc = {
        "change_id": str(uuid.uuid5(uuid.NAMESPACE_URL, case + "/document")),
        "node_change_id": nodes[0]["change_id"], "content_hash": DIGEST,
        "original_filename": "existing-decision.bin", "storage_filename": DIGEST,
        "mime_type": "application/octet-stream", "file_size": len(CONTENT),
        "description": "A document attached before the upgrade", "attached_by": "release-test",
        "attached_at": "2025-04-04T12:23:34Z",
    }
    status, _, body = request("/blob/" + DIGEST, "PUT", CONTENT)
    assert status == 200, (status, body)
    payload = {"workspace": workspace, "graph": {"nodes": nodes, "edges": edges, "documents": [doc]}}
    for _ in range(2):
        status, _, body = request("/import", "POST", payload)
        assert status == 200, (status, body)
        report = json.loads(body)
        assert report["edges"]["unresolved"] == 0, report
    graph = export(workspace)
    assert len(graph["nodes"]) == 300 and len(graph["edges"]) == 299 and len(graph["documents"]) == 1
    client = MCP()
    result = client.tool("query_nodes", {"workspace": workspace, "limit": 10})
    assert result["count"] == 10
    state = {"graph": graph, "session": client.session, "workspace": workspace}
    Path(f"/state/{case}.json").write_text(json.dumps(state, sort_keys=True))
    print(f"PASS {case}: imported 300 nodes, 299 edges and binary document twice; queried through MCP", flush=True)


def verify(case, *, replaced=False):
    state = json.loads(Path(f"/state/{case}.json").read_text())
    actual = export(state["workspace"])
    assert actual == state["graph"], "Graph IDs, text, metadata, edges, dates or documents changed"
    doc_id = actual["documents"][0]["id"]
    status, _, body = request("/documents/" + doc_id)
    assert status == 200 and body == CONTENT, (status, body)
    for path in ("/mcp", "/export?workspace=" + state["workspace"], "/documents/" + doc_id):
        assert request(path, token=None)[0] == 401, path
        assert request(path, token="incorrect-token")[0] == 401, path
    if replaced:
        status, _, body = request("/mcp", "POST", {
            "jsonrpc": "2.0", "id": 42, "method": "tools/list", "params": {},
        }, headers={"Mcp-Session-Id": state["session"]})
        assert status == 404 and decoded(body)["error"]["code"] == -32001, (status, body)
    a, b = MCP("acceptance-a-" + case), MCP("acceptance-b-" + case)
    suffix = uuid.uuid4().hex
    goal = a.tool("add_node", {"node_type": "goal", "title": "Goal " + suffix, "branch": suffix})
    action = a.tool("add_node", {"node_type": "action", "title": "Action " + suffix, "branch": suffix})
    edge = a.tool("add_edge", {"from_node_id": goal["id"], "to_node_id": action["id"], "branch": suffix})
    assert edge["id"]
    foreign = b.tool("add_node", {"node_type": "goal", "title": "Other " + suffix, "branch": suffix})
    unpinned = MCP()
    queries = [(a, {}), (b, {}), (unpinned, {"workspace": "acceptance-b-" + case})]
    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
        results = list(pool.map(lambda pair: pair[0].tool("query_nodes", pair[1]), queries))
    assert goal["id"] in {n["id"] for n in results[0]["nodes"]}
    assert foreign["id"] not in {n["id"] for n in results[0]["nodes"]}
    assert goal["id"] not in {n["id"] for n in results[1]["nodes"]}
    assert foreign["id"] in {n["id"] for n in results[1]["nodes"]}
    assert foreign["id"] in {n["id"] for n in results[2]["nodes"]}
    assert goal["id"] not in {n["id"] for n in results[2]["nodes"]}
    events = Events("acceptance-a-" + case)
    try:
        event_node = a.tool("add_node", {"node_type": "observation", "title": "Live " + suffix, "branch": suffix})
        event = events.read()
        # Permit a subscription acknowledgement if the transport adds one.
        if event.get("type") == "subscribed":
            event = events.read()
        assert event["id"] == event_node["id"] and event["title"] == "Live " + suffix, event
    finally:
        events.close()
    print(f"PASS {case}: preserved full graph/document; auth, MCP writes, concurrent workspace isolation and live events", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["wait", "seed", "verify"])
    parser.add_argument("--case", default="fresh")
    parser.add_argument("--path", default="/ready")
    parser.add_argument("--status", type=int, default=200)
    parser.add_argument("--replaced", action="store_true")
    args = parser.parse_args()
    if args.mode == "wait":
        wait(args.path, args.status)
    elif args.mode == "seed":
        seed(args.case)
    else:
        verify(args.case, replaced=args.replaced)
