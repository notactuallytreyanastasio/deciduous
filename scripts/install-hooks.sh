#!/usr/bin/env bash
#
# install-hooks.sh - Install git hooks for deciduous
#
# This script installs git hooks to enforce type synchronization
# and other checks before commits and pushes.
#
# Usage:
#   ./scripts/install-hooks.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
HOOKS_DIR="$PROJECT_ROOT/.git/hooks"

echo "Installing git hooks..."

# Create pre-commit hook
cat > "$HOOKS_DIR/pre-commit" << 'EOF'
#!/usr/bin/env bash
#
# pre-commit hook - Run clippy and validate types before committing
#

set -euo pipefail

# Get the root of the repository
REPO_ROOT="$(git rev-parse --show-toplevel)"

# Run clippy first (catches lint errors)
echo "Running clippy..."
if ! cargo clippy --quiet 2>&1; then
    echo ""
    echo "ERROR: Clippy found issues. Commit aborted."
    echo "Run 'cargo clippy' to see details."
    exit 1
fi

echo "Running type validation..."
if ! "$REPO_ROOT/scripts/validate-types.sh"; then
    echo ""
    echo "ERROR: Type validation failed. Commit aborted."
    echo "Fix the type mismatches before committing."
    exit 1
fi

# Check if any type-related files are staged
TYPE_FILES_STAGED=$(git diff --cached --name-only | grep -E '(src/db\.rs|src/tui/types\.rs|web/src/types/graph\.ts|schema/decision-graph\.schema\.json)' || true)

if [[ -n "$TYPE_FILES_STAGED" ]]; then
    echo "Type-related files changed:"
    echo "$TYPE_FILES_STAGED"
    echo ""

    # Re-run validation to ensure all staged type changes are in sync
    if ! "$REPO_ROOT/scripts/validate-types.sh"; then
        echo ""
        echo "ERROR: Staged type changes are out of sync. Commit aborted."
        exit 1
    fi
fi

# Check if web source files changed - demo must be rebuilt
WEB_SRC_STAGED=$(git diff --cached --name-only | grep -E '^web/src/' || true)
DEMO_STAGED=$(git diff --cached --name-only | grep -E '^docs/demo/index\.html$' || true)

if [[ -n "$WEB_SRC_STAGED" ]]; then
    echo "Web source files changed:"
    echo "$WEB_SRC_STAGED"
    echo ""

    if [[ -z "$DEMO_STAGED" ]]; then
        echo "ERROR: Web source changed but docs/demo/index.html not staged!"
        echo ""
        echo "You must rebuild and commit the demo:"
        echo "  ./scripts/build-demo.sh"
        echo "  git add docs/demo/index.html"
        echo ""
        exit 1
    fi

    # Verify the staged demo matches a fresh build
    echo "Verifying demo build is current..."
    TEMP_BUILD=$(mktemp)
    (cd "$REPO_ROOT/web" && npm run build --silent 2>/dev/null)
    cp "$REPO_ROOT/web/dist/index.html" "$TEMP_BUILD"

    # Compare staged demo with fresh build
    STAGED_DEMO=$(git show :docs/demo/index.html 2>/dev/null || cat "$REPO_ROOT/docs/demo/index.html")
    FRESH_BUILD=$(cat "$TEMP_BUILD")
    rm -f "$TEMP_BUILD"

    if [[ "$STAGED_DEMO" != "$FRESH_BUILD" ]]; then
        echo ""
        echo "ERROR: Staged docs/demo/index.html doesn't match fresh build!"
        echo ""
        echo "Rebuild and re-stage:"
        echo "  ./scripts/build-demo.sh"
        echo "  git add docs/demo/index.html"
        echo ""
        exit 1
    fi
    echo "✓ Demo build is current"
fi

echo "Pre-commit checks passed."
EOF
chmod +x "$HOOKS_DIR/pre-commit"
echo "✓ Installed pre-commit hook"

# Create pre-push hook (only enforces strict checks for main/master)
cat > "$HOOKS_DIR/pre-push" << 'EOF'
#!/usr/bin/env bash
#
# pre-push hook - Validate types and tests before pushing to main
#
# For feature branches: No blocking checks (WIP pushes are fine)
# For main/master: Full validation suite must pass
#

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"

# Read the remote and URL from stdin
while read local_ref local_sha remote_ref remote_sha; do
    # Only enforce strict checks for main/master
    if [[ "$remote_ref" == "refs/heads/main" || "$remote_ref" == "refs/heads/master" ]]; then
        echo "=============================================="
        echo "Pushing to protected branch: $remote_ref"
        echo "Running full validation suite..."
        echo "=============================================="
        echo ""

        # Full validation suite for main branch
        echo "1/3 Running type validation..."
        if ! "$REPO_ROOT/scripts/validate-types.sh"; then
            echo ""
            echo "ERROR: Type validation failed. Push to main aborted."
            echo "Run './scripts/validate-types.sh' to see details."
            exit 1
        fi

        echo ""
        echo "2/3 Running tests..."
        if ! cargo test --quiet; then
            echo ""
            echo "ERROR: Tests failed. Push to main aborted."
            exit 1
        fi

        echo ""
        echo "3/3 Building web UI..."
        if ! (cd "$REPO_ROOT/web" && npm run build --silent 2>/dev/null); then
            echo ""
            echo "ERROR: Web build failed. Push to main aborted."
            exit 1
        fi

        echo ""
        echo "=============================================="
        echo "All checks passed. Proceeding with push."
        echo "=============================================="
    fi
    # Feature branches: no checks, allow WIP pushes
done

exit 0
EOF
chmod +x "$HOOKS_DIR/pre-push"
echo "✓ Installed pre-push hook (only enforces for main/master)"

echo ""
echo "Git hooks installed successfully!"
echo ""
echo "Hooks installed:"
echo "  - pre-commit: Runs clippy + validates types + enforces demo build sync"
echo "  - pre-push: Full validation (types + tests + build) for main branch"
