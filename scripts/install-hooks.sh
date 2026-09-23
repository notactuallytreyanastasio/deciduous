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
# pre-commit hook - Run fmt, clippy and validate types before committing
#

set -euo pipefail

# Get the root of the repository
REPO_ROOT="$(git rev-parse --show-toplevel)"

# Run cargo fmt check first (code must be formatted)
echo "Checking code formatting..."
if ! cargo fmt --check --quiet 2>&1; then
    echo ""
    echo "ERROR: Code is not formatted. Commit aborted."
    echo "Run 'cargo fmt' to format the code."
    exit 1
fi

# Run clippy (catches lint errors)
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
        echo "1/6 Checking code formatting..."
        if ! cargo fmt --check --quiet 2>&1; then
            echo ""
            echo "ERROR: Code is not formatted. Push to main aborted."
            echo "Run 'cargo fmt' to format the code."
            exit 1
        fi

        echo ""
        echo "2/6 Running type validation..."
        if ! "$REPO_ROOT/scripts/validate-types.sh"; then
            echo ""
            echo "ERROR: Type validation failed. Push to main aborted."
            echo "Run './scripts/validate-types.sh' to see details."
            exit 1
        fi

        echo ""
        echo "3/6 Running tests..."
        if ! cargo test --quiet; then
            echo ""
            echo "ERROR: Tests failed. Push to main aborted."
            exit 1
        fi

        echo ""
        echo "4/6 Building web UI..."
        if ! (cd "$REPO_ROOT/web" && npm run build --silent 2>/dev/null); then
            echo ""
            echo "ERROR: Web build failed. Push to main aborted."
            exit 1
        fi

        echo ""
        echo "5/6 Checking viewer.html sync..."
        # Build fresh and compare with committed viewer.html
        FRESH_BUILD="$REPO_ROOT/web/dist/index.html"
        VIEWER_HTML="$REPO_ROOT/src/viewer.html"

        if [ -f "$FRESH_BUILD" ] && [ -f "$VIEWER_HTML" ]; then
            FRESH_HASH=$(shasum -a 256 "$FRESH_BUILD" | cut -d' ' -f1)
            VIEWER_HASH=$(shasum -a 256 "$VIEWER_HTML" | cut -d' ' -f1)

            if [ "$FRESH_HASH" != "$VIEWER_HASH" ]; then
                echo ""
                echo "ERROR: src/viewer.html is out of sync with web build!"
                echo ""
                echo "The embedded viewer for 'deciduous serve' doesn't match the latest web UI."
                echo "This means cargo install users would get an outdated UI."
                echo ""
                echo "To fix, run:"
                echo "  cp web/dist/index.html src/viewer.html"
                echo "  git add src/viewer.html"
                echo "  git commit --amend --no-edit"
                echo ""
                exit 1
            fi
            echo "✓ viewer.html matches fresh web build"
        fi

        echo ""
        echo "6/6 Checking the decision graph file is committed..."
        if command -v deciduous &> /dev/null; then
            deciduous sync > /dev/null 2>&1 || true

            # The graph file is how teammates receive decisions; pushing without it is a silent loss
            if [ -n "$(git -C "$REPO_ROOT" status --porcelain -- .deciduous/graph.json 2>/dev/null)" ]; then
                echo ""
                echo "WARNING: .deciduous/graph.json has uncommitted graph records."
                echo "  git add .deciduous/graph.json && git commit -m 'graph: sync records'"
            fi
            echo "✓ Decision graph checked"
        else
            echo "⚠ deciduous not found, skipping graph sync check"
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
echo "  - pre-commit: Runs fmt + clippy + validates types + enforces demo build sync"
echo "  - pre-push: Full validation (fmt + types + tests + build + graph sync) for main branch"
