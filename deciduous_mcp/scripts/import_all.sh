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
ok=0; failed=0; nodes=0; edges=0; unresolved=0

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
    read -r n e u < <(python3 -c "
import json; d=json.load(open('$tmp/resp.json'))
print(d['nodes']['upserted'], d['edges']['upserted'], d['edges']['unresolved'])")
    printf '%-34s %5s nodes %5s edges' "$ws" "$n" "$e"
    [ "$u" != 0 ] && printf '  (%s edges unresolved)' "$u"
    printf '\n'
    ok=$((ok + 1)); nodes=$((nodes + n)); edges=$((edges + e)); unresolved=$((unresolved + u))
  else
    printf '%-34s HTTP %s: %s\n' "$ws" "$code" "$(head -c 300 "$tmp/resp.json")"
    failed=$((failed + 1))
  fi
done < <(find "$ROOT" -maxdepth 4 -name deciduous.db -path '*/.deciduous/*' -not -path '*/node_modules/*' 2>/dev/null | sort)

echo
echo "imported $ok graphs, $failed failed — $nodes nodes, $edges edges, $unresolved edges unresolved"
[ "$unresolved" -gt 0 ] && echo "unresolved edges are self-loops and dangling endpoints the schema rejects; they are reported per project above"
exit 0
