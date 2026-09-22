#!/usr/bin/env bash
# Push every deciduous graph on this machine into the shared Postgres.
#
# Workspace naming follows the rule the server expects: a graph inside a git
# repo is named for that repo's root directory, and anything outside one pools
# into "scratch". Worktrees resolve to their own root, so sibling worktrees of
# one repo stay separate workspaces rather than colliding.
#
# The import is idempotent on [workspace_id, change_id], so re-running it is
# the normal way to refresh, not a mistake.
#
#   DECIDUOUS_MCP_URL=https://... DECIDUOUS_MCP_TOKEN=... ./import_all.sh [root]
#
#   -n   dry run: print what would be sent, send nothing
set -uo pipefail

ROOT="${1:-$HOME/code}"
URL="${DECIDUOUS_MCP_URL:?set DECIDUOUS_MCP_URL, e.g. https://deciduous-mcp.bobbby.online}"
TOKEN="${DECIDUOUS_MCP_TOKEN:?set DECIDUOUS_MCP_TOKEN}"
DRY=0
[ "${1:-}" = "-n" ] && { DRY=1; ROOT="${2:-$HOME/code}"; }

command -v deciduous >/dev/null || { echo "deciduous not on PATH" >&2; exit 1; }

tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
ok=0; failed=0; nodes=0; edges=0; unresolved=0; blobs=0; dupes=0

# --- Phase 1: document bytes -------------------------------------------------
#
# Every file in every .deciduous/documents/ directory is uploaded under its own
# computed sha256, regardless of which database references it. Uploading
# per-database instead misses files that have drifted: deep-squishing-sparrow.md
# lives in njlegalize-me/.deciduous/documents/ but the only row referencing it
# is in a different graph entirely, whose own documents directory does not
# exist. A content-addressed sweep recovers those; a per-database walk cannot
# see them.
#
# Bytes go up before metadata so that `content_missing` is right the first
# time. The server verifies each hash, so a corrupted or renamed file is
# rejected rather than silently shadowing another document's content.
if [ "$DRY" != 1 ]; then
  echo "uploading document content..."
  while IFS= read -r file; do
    hash=$(shasum -a 256 "$file" | cut -d' ' -f1)
    case "$(basename "$file")" in
      *.pdf)  mime=application/pdf ;;
      *.md)   mime=text/markdown ;;
      *.txt)  mime=text/plain ;;
      *.png)  mime=image/png ;;
      *.jpg|*.jpeg) mime=image/jpeg ;;
      *)      mime=application/octet-stream ;;
    esac
    bcode=$(curl -sS -o "$tmp/blob.json" -w '%{http_code}' -X PUT "$URL/blob/$hash" \
      -H "authorization: Bearer $TOKEN" -H "content-type: $mime" --data-binary "@$file")
    if [ "$bcode" = 200 ]; then
      blobs=$((blobs + 1))
    else
      printf '  blob HTTP %s  %s\n    %s\n' "$bcode" "$file" "$(head -c 200 "$tmp/blob.json")"
    fi
  done < <(find "$ROOT" -maxdepth 5 -type f -path '*/.deciduous/documents/*' -not -path '*/node_modules/*' 2>/dev/null | sort)
  echo "  $blobs files uploaded"
  echo
fi

# --- Phase 2: graphs ---------------------------------------------------------

while IFS= read -r db; do
  dir=$(dirname "$(dirname "$db")")          # .../<project>/.deciduous/deciduous.db

  # Workspace name: git repo root basename, else the pool.
  if root=$(git -C "$dir" rev-parse --show-toplevel 2>/dev/null); then
    ws=$(basename "$root")
  else
    ws=scratch
  fi
  ws=$(printf '%s' "$ws" | tr '[:upper:]' '[:lower:]')

  # `deciduous graph` is the only export that survived both sync rewrites and
  # it is the thing that knows how to read an older database, so the CLI does
  # the reading rather than this script poking at SQLite.
  if ! DECIDUOUS_DB_PATH="$db" deciduous graph > "$tmp/graph.json" 2>"$tmp/err"; then
    printf '%-34s FAILED to export: %s\n' "$ws" "$(head -1 "$tmp/err")"
    failed=$((failed + 1)); continue
  fi

  python3 - "$tmp/graph.json" "$tmp/payload.json" "$ws" "$dir" <<'PY'
import json, sys
graph = json.load(open(sys.argv[1]))
json.dump({"workspace": sys.argv[3], "source": sys.argv[4], "graph": graph},
          open(sys.argv[2], "w"))
PY

  if [ "$DRY" = 1 ]; then
    printf '%-34s would send %s\n' "$ws" \
      "$(python3 -c "import json;g=json.load(open('$tmp/graph.json'));print(len(g['nodes']),'nodes',len(g['edges']),'edges')")"
    continue
  fi

  code=$(curl -sS -o "$tmp/resp.json" -w '%{http_code}' -X POST "$URL/import" \
    -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' \
    --data-binary "@$tmp/payload.json")

  if [ "$code" = 200 ]; then
    read -r n e u dn dm < <(python3 -c "
import json; d=json.load(open('$tmp/resp.json'))
doc=d.get('documents') or {}
print(d['nodes']['upserted'], d['edges']['upserted'], d['edges']['unresolved'],
      doc.get('upserted',0), doc.get('content_missing',0))")
    printf '%-34s %5s nodes %5s edges' "$ws" "$n" "$e"
    [ "$dn" != 0 ] && printf ' %3s docs' "$dn"
    [ "$u" != 0 ] && printf '  (%s edges unresolved)' "$u"
    [ "$dm" != 0 ] && printf '  (%s docs with no content)' "$dm"
    printf '\n'
    ok=$((ok + 1)); nodes=$((nodes + n)); edges=$((edges + e)); unresolved=$((unresolved + u))
  else
    printf '%-34s HTTP %s: %s\n' "$ws" "$code" "$(head -c 300 "$tmp/resp.json")"
    failed=$((failed + 1))
  fi
done < <(find "$ROOT" -maxdepth 4 -name deciduous.db -path '*/.deciduous/*' -not -path '*/node_modules/*' 2>/dev/null | sort)

echo
echo "imported $ok graphs, $failed failed — $nodes nodes, $edges edges, $unresolved edges unresolved"
echo "documents: $blobs files uploaded (deduplicated server-side by content hash)"
[ "$unresolved" -gt 0 ] && echo "unresolved edges are self-loops and dangling endpoints the schema rejects; they are reported per project above"
exit 0
