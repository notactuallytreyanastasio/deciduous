"""Small standard-library helpers. Never log bearer tokens or database URLs."""

import json
import os
from pathlib import Path
import re
import stat
import urllib.error
import urllib.parse
import urllib.request


class SafetyError(Exception):
    pass


def private_directory(path):
    path = Path(path).expanduser().absolute()
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
    if path.is_symlink() or not path.is_dir():
        raise SafetyError("Expected a real directory, not a symlink")
    if stat.S_IMODE(path.stat().st_mode) & 0o077:
        raise SafetyError(f"Directory must be owner-only: chmod 700 {path}")
    return path


def private_write(path, content):
    """Create without overwriting; mode is private before any bytes are written."""
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
    with os.fdopen(os.open(path, flags, 0o600), "wb") as stream:
        stream.write(content if isinstance(content, bytes) else content.encode())


def read_private(path):
    path = Path(path).expanduser()
    if path.is_symlink() or not path.is_file():
        raise SafetyError("Secret file must exist and must not be a symlink")
    if stat.S_IMODE(path.stat().st_mode) & 0o077:
        raise SafetyError(f"Secret file must be owner-only: chmod 600 {path}")
    return path.read_text()


def token_from_environment():
    token = os.environ.get("DECIDUOUS_MCP_TOKEN", "").strip()
    if not token:
        config = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config"))
        token = read_private(config / "deciduous" / "credentials").strip()
    if len(token.encode()) < 32 or "\n" in token or "\r" in token:
        raise SafetyError("Set a valid DECIDUOUS_MCP_TOKEN or run deciduous remote login")
    return token


def base_url(value):
    parsed = urllib.parse.urlsplit(value)
    if (not parsed.hostname or parsed.username or parsed.password
            or parsed.query or parsed.fragment):
        raise SafetyError("Use a base URL without credentials, query, or fragment")
    loopback = parsed.hostname in ("localhost", "127.0.0.1", "::1")
    if parsed.scheme != "https" and not (parsed.scheme == "http" and loopback):
        raise SafetyError("Use HTTPS, except for HTTP on a loopback address")
    if parsed.path.rstrip("/").endswith("/mcp"):
        raise SafetyError("Use the server base URL, without the /mcp transport suffix")
    return value.rstrip("/")


def workspace_name(value):
    if not re.fullmatch(r"[a-z0-9][a-z0-9._-]{0,127}", value):
        raise SafetyError("Choose a lowercase workspace slug, for example example-app")
    return value


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise SafetyError("Refusing an HTTP redirect; verify the server base URL")


def request(url, token=None, data=None, method=None, content_type="application/json", timeout=600):
    headers = {"Accept": "application/json"}
    if token:
        headers["Authorization"] = "Bearer " + token
    if data is not None:
        headers["Content-Type"] = content_type
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.build_opener(NoRedirect).open(req, timeout=timeout) as response:
            return response.read()
    except urllib.error.HTTPError as error:
        # Error bodies can contain graph content or proxy debug information.
        raise SafetyError(f"Server returned HTTP {error.code}; no credentials printed") from None
    except urllib.error.URLError:
        raise SafetyError("Cannot reach server; check URL, certificate, and network") from None


def get_graph(url, token, workspace, timeout=300):
    query = urllib.parse.urlencode({"workspace": workspace})
    try:
        graph = json.loads(request(url + "/export?" + query, token, timeout=timeout))
    except (ValueError, TypeError):
        raise SafetyError("Server did not return graph JSON") from None
    if not isinstance(graph, dict) or not isinstance(graph.get("nodes"), list):
        raise SafetyError("Server did not return a graph; check the base URL")
    return graph
