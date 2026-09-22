"""Reproducible public helper bundle. Never include state, credentials, or tests."""
import hashlib
import io
from pathlib import Path
import sys
import zipfile

root = Path(__file__).resolve().parents[2]
names = ["common.py", "local-stack.py", "migrate-sqlite.py", "compose.yaml", "compose.remote.example.yaml", "Caddyfile.example", "README.md", "postgres-only.py", "postgres-only/Dockerfile", "postgres-only/compose.yaml", "postgres-only/README.md", "postgres-only/.gitignore"]
buffer = io.BytesIO()
with zipfile.ZipFile(buffer, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
    for name in names:
        info = zipfile.ZipInfo("scripts/team-memory/" + name, (2026, 9, 22, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        info.external_attr = 0o100644 << 16
        archive.writestr(info, (root / "scripts/team-memory" / name).read_bytes())
payload = buffer.getvalue()
outputs = {
    "team-memory.zip": payload,
    "team-memory.zip.sha256": (hashlib.sha256(payload).hexdigest() + "  team-memory.zip\n").encode(),
}
for name, data in outputs.items():
    destination = root / "docs/downloads" / name
    if "--check" in sys.argv:
        if not destination.exists() or destination.read_bytes() != data:
            raise SystemExit("Out of date: " + str(destination))
    else:
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
print("Checked helper bundle" if "--check" in sys.argv else "Built helper bundle")
