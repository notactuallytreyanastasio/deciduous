#!/usr/bin/env python3
"""Boot a native release with its embedded BEAM and an unavailable database."""

import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request


def isolated_environment(directory, port):
    # Keep the OS environment needed to launch native executables, but do not
    # inherit developer BEAM flags, database settings, or HTTP proxies.
    keep = {"SYSTEMROOT", "WINDIR", "COMSPEC", "SYSTEMDRIVE", "USERPROFILE", "HOME"}
    env = {key: value for key, value in os.environ.items() if key.upper() in keep}
    if os.name == "nt":
        system_root = os.environ["SystemRoot"]
        env["PATH"] = str(Path(system_root) / "System32")
    else:
        # Some hosted runners preinstall BEAM in /usr/bin. The native wrapper
        # launches its bundled erlexec by absolute path, so no helpers belong
        # on PATH during this test.
        empty_path = directory / "empty-path"
        empty_path.mkdir()
        env["PATH"] = str(empty_path)
    env.update(
        DATABASE_URL="ecto://native_smoke:unused@127.0.0.1:1/native_smoke",
        DECIDUOUS_MCP_TOKEN="native-smoke-only-0123456789abcdef0123456789abcdef",
        DECIDUOUS_MCP_INSTALL_DIR=str(directory / "runtime"),
        APPDATA=str(directory / "appdata"),
        LOCALAPPDATA=str(directory / "appdata"),
        TEMP=str(directory),
        TMP=str(directory),
        TMPDIR=str(directory),
        PORT=str(port),
        POOL_SIZE="1",
        DB_SSL="false",
    )
    for command in ("erl", "elixir", "mix"):
        if shutil.which(command, path=env["PATH"]):
            raise RuntimeError(f"{command} is unexpectedly available in the isolated PATH")
    return env


def stop_process(process):
    if process.poll() is not None:
        return
    if os.name == "nt":
        # Burrito's Windows wrapper launches BEAM as a child process. Kill the
        # whole test process tree so the VM cannot outlive the smoke test.
        taskkill = Path(os.environ["SystemRoot"]) / "System32" / "taskkill.exe"
        subprocess.run(
            [str(taskkill), "/PID", str(process.pid), "/T", "/F"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=10,
            check=False,
        )
    else:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            return
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        if os.name != "nt":
            os.killpg(process.pid, signal.SIGKILL)
        else:
            process.kill()
        process.wait(timeout=10)


def response_status(opener, url):
    try:
        with opener.open(url, timeout=2) as response:
            return response.status
    except urllib.error.HTTPError as error:
        return error.code
    except (urllib.error.URLError, TimeoutError, ConnectionError):
        return None


def smoke(executable, timeout):
    with tempfile.TemporaryDirectory(prefix="deciduous-native-smoke-") as temp:
        directory = Path(temp)
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        env = isolated_environment(directory, port)
        metadata = subprocess.run(
            [str(executable), "maintenance", "meta"],
            env=env,
            cwd=directory,
            capture_output=True,
            text=True,
            timeout=30,
            check=True,
        )
        # Burrito may log the private install-directory override before JSON.
        metadata_lines = [line for line in metadata.stdout.splitlines() if line.startswith("{")]
        if len(metadata_lines) != 1:
            raise RuntimeError(f"Missing runtime metadata: {metadata.stdout}")
        info = json.loads(metadata_lines[0])
        if info.get("app_name") != "deciduous_mcp" or not info.get("erts_version"):
            raise RuntimeError(f"Unexpected embedded runtime metadata: {info}")

        log_path = directory / "server.log"
        process_options = (
            {"creationflags": subprocess.CREATE_NEW_PROCESS_GROUP}
            if os.name == "nt"
            else {"start_new_session": True}
        )
        with log_path.open("wb") as log:
            process = subprocess.Popen(
                [str(executable)],
                cwd=directory,
                env=env,
                stdout=log,
                stderr=subprocess.STDOUT,
                **process_options,
            )
            try:
                opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
                deadline = time.monotonic() + timeout
                healthy_checks = 0
                while time.monotonic() < deadline:
                    if process.poll() is not None:
                        raise RuntimeError(f"Native server exited with code {process.returncode}")
                    health = response_status(opener, f"http://127.0.0.1:{port}/health")
                    ready = response_status(opener, f"http://127.0.0.1:{port}/ready")
                    healthy_checks = healthy_checks + 1 if (health, ready) == (200, 503) else 0
                    if healthy_checks == 6:
                        print(
                            f"PASS {executable.name}: embedded ERTS {info['erts_version']}; "
                            "no host BEAM; /health=200; /ready=503 without PostgreSQL"
                        )
                        return
                    time.sleep(0.5)
                raise RuntimeError(f"Timed out: last /health={health}, /ready={ready}")
            except Exception:
                log.flush()
                print(log_path.read_text(errors="replace")[-12000:], file=sys.stderr)
                raise
            finally:
                stop_process(process)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("--timeout", type=float, default=90)
    args = parser.parse_args()
    executable = args.executable.resolve(strict=True)
    if not executable.is_file():
        parser.error("executable must be a native release file")
    if args.timeout <= 0:
        parser.error("timeout must be positive")
    smoke(executable, args.timeout)


if __name__ == "__main__":
    main()
