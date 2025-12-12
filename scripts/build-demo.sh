#!/usr/bin/env bash
#
# build-demo.sh - Build and deploy web viewer to docs/demo for GitHub Pages
#
# Usage:
#   ./scripts/build-demo.sh
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "Building web viewer..."
cd "$PROJECT_ROOT/web"
npm run build

echo "Deploying to docs/demo/..."
# Copy the single-file build (all JS/CSS inlined into index.html)
cp "$PROJECT_ROOT/web/dist/index.html" "$PROJECT_ROOT/docs/demo/index.html"

# Sync graph data
echo "Syncing graph data..."
cd "$PROJECT_ROOT"
deciduous sync

echo ""
echo "✓ Demo deployed to docs/demo/"
echo "  - index.html: $(wc -c < "$PROJECT_ROOT/docs/demo/index.html" | tr -d ' ') bytes"
echo "  - graph-data.json: $(wc -c < "$PROJECT_ROOT/docs/demo/graph-data.json" | tr -d ' ') bytes"
