#!/usr/bin/env python3
"""Generate a new Postgres-only Docker project. Does not start Docker."""

import argparse
import os
from pathlib import Path
import secrets
import sys

from common import SafetyError

TEMPLATES = Path(__file__).resolve().parent / "postgres-only"
FILES = ("Dockerfile", "compose.yaml", "README.md", ".gitignore")


def generate(output, port=55432):
    if not 1024 <= port <= 65535:
        raise SafetyError("Port must be between 1024 and 65535")
    output = Path(output).expanduser().absolute()
    # lexists catches dangling symlinks; never resolve a selected target before
    # checking it, because that would hide the link the caller gave us.
    if os.path.lexists(output):
        raise SafetyError("Output already exists. Choose a new directory; nothing overwritten")
    if output.parent.is_symlink() or not output.parent.is_dir():
        raise SafetyError("Output parent must be an existing real directory, not a symlink")
    templates = {name: (TEMPLATES / name).read_bytes() for name in FILES}
    password = secrets.token_hex(32)
    environment = ("POSTGRES_DB=deciduous\nPOSTGRES_USER=deciduous\n"
                   f"POSTGRES_PASSWORD={password}\nPOSTGRES_PORT={port}\n").encode()
    # mkdir is exclusive: a concurrent creator is not overwritten either.
    output.mkdir(mode=0o700)
    created = []
    try:
        for name, content in [(".env", environment), *templates.items()]:
            path = output / name
            descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            created.append(path)
            with os.fdopen(descriptor, "wb") as stream:
                stream.write(content)
    except OSError:
        # Remove only files this invocation created, then the empty directory.
        # No recursive cleanup: unexpected files are somebody else's work.
        for path in reversed(created):
            try:
                path.unlink()
            except OSError:
                pass
        try:
            output.rmdir()
        except OSError:
            pass
        raise
    return output


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("deciduous-postgres"))
    parser.add_argument("--port", type=int, default=55432)
    args = parser.parse_args(argv)
    output = generate(args.output, args.port)
    print(f"Created Postgres-only project: {output}")
    print(f"Database/user: deciduous; loopback port: 127.0.0.1:{args.port}")
    print("Private .env written; password not printed. No containers started.")
    print("From that directory: docker compose up -d --build --wait")
    print("Read README.md to connect a separate MCP service and preserve the volume.")


if __name__ == "__main__":
    try:
        main()
    except (SafetyError, OSError) as error:
        print(f"Stopped: {error}", file=sys.stderr)
        sys.exit(1)
