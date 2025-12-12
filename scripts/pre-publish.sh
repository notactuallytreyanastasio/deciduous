#!/usr/bin/env bash
#
# pre-publish.sh - Validate everything before publishing to crates.io
#
# This script runs all validation checks required before publishing.
# It should be run before `cargo publish`.
#
# Usage:
#   ./scripts/pre-publish.sh
#
# Exit codes:
#   0 - All checks passed, safe to publish
#   1 - Validation failed, do not publish

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log() {
    echo -e "${BLUE}==>${NC} $1"
}

success() {
    echo -e "${GREEN}✓${NC} $1"
}

error() {
    echo -e "${RED}✗${NC} $1" >&2
}

warn() {
    echo -e "${YELLOW}⚠${NC} $1"
}

echo ""
echo "=============================================="
echo "  Pre-Publish Validation"
echo "=============================================="
echo ""

ERRORS=0

# 1. Type synchronization
log "Checking type synchronization..."
if "$SCRIPT_DIR/validate-types.sh"; then
    success "Type synchronization"
else
    error "Type synchronization failed"
    ((ERRORS++))
fi
echo ""

# 2. Rust tests
log "Running Rust tests..."
if cargo test --quiet 2>/dev/null; then
    success "Rust tests"
else
    error "Rust tests failed"
    ((ERRORS++))
fi
echo ""

# 3. Clippy
log "Running Clippy..."
if cargo clippy -- -D warnings 2>/dev/null; then
    success "Clippy"
else
    error "Clippy found issues"
    ((ERRORS++))
fi
echo ""

# 4. TypeScript build
log "Building TypeScript web UI..."
if (cd "$PROJECT_ROOT/web" && npm run build --silent 2>/dev/null); then
    success "TypeScript build"
else
    error "TypeScript build failed"
    ((ERRORS++))
fi
echo ""

# 5. TypeScript type check
log "Running TypeScript type check..."
if (cd "$PROJECT_ROOT/web" && npx tsc --noEmit 2>/dev/null); then
    success "TypeScript type check"
else
    error "TypeScript type check failed"
    ((ERRORS++))
fi
echo ""

# 6. Check for uncommitted changes
log "Checking for uncommitted changes..."
if git diff --quiet && git diff --staged --quiet; then
    success "No uncommitted changes"
else
    warn "You have uncommitted changes"
    git status --short
fi
echo ""

# 7. Check if on main branch
log "Checking branch..."
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [[ "$CURRENT_BRANCH" == "main" || "$CURRENT_BRANCH" == "master" ]]; then
    success "On main branch: $CURRENT_BRANCH"
else
    warn "Not on main branch (current: $CURRENT_BRANCH)"
    echo "  Consider publishing from main branch"
fi
echo ""

# Summary
echo "=============================================="
if [[ $ERRORS -eq 0 ]]; then
    echo -e "${GREEN}All pre-publish checks passed!${NC}"
    echo ""
    echo "You can now run:"
    echo "  cargo publish"
    echo ""
    exit 0
else
    echo -e "${RED}$ERRORS check(s) failed. Do not publish.${NC}"
    echo ""
    echo "Fix the issues above before publishing."
    echo ""
    exit 1
fi
