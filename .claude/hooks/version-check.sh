#!/bin/bash
# version-check.sh
# Checks for new deciduous versions via crates.io (opt-in, once per 24h)
# Non-blocking: exit 0 always, informational only

# Check if deciduous is initialized
if [ ! -d ".deciduous" ]; then
    exit 0
fi

# Check if auto_check is enabled in config
config_file=".deciduous/config.toml"
if [ ! -f "$config_file" ]; then
    exit 0
fi

# Simple grep for auto_check = true (handles spaces around =)
if ! grep -qE '^\s*auto_check\s*=\s*true' "$config_file" 2>/dev/null; then
    exit 0
fi

# Rate limit: only check once per 24 hours
check_file=".deciduous/.last_version_check"
if [ -f "$check_file" ]; then
    last_check=$(cat "$check_file" 2>/dev/null || echo "0")
    now=$(date +%s)
    elapsed=$((now - last_check))
    # 86400 seconds = 24 hours
    if [ "$elapsed" -lt 86400 ]; then
        # Already checked recently - but still report cached result if newer
        cached_file=".deciduous/.latest_version"
        if [ -f "$cached_file" ]; then
            latest=$(cat "$cached_file" 2>/dev/null)
            current=$(deciduous --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)
            if [ -n "$latest" ] && [ -n "$current" ] && [ "$latest" != "$current" ]; then
                cat >&2 << EOF
DECIDUOUS: New version available (v${current} -> v${latest})
Run 'deciduous check-update' for details, then 'cargo install deciduous' to upgrade.
After upgrading, run 'deciduous update' to refresh integration files.
Please let the user know a new version is available.
EOF
            fi
        fi
        exit 0
    fi
fi

# Fetch latest version from crates.io (3 second timeout)
latest=$(curl -s --max-time 3 "https://crates.io/api/v1/crates/deciduous" 2>/dev/null | grep -oE '"max_version":"[^"]*"' | head -1 | sed 's/"max_version":"//;s/"//')

if [ -z "$latest" ]; then
    # Network error or timeout - skip silently
    exit 0
fi

# Cache the result
echo "$latest" > ".deciduous/.latest_version"
date +%s > "$check_file"

# Compare with current version
current=$(deciduous --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)

if [ -n "$current" ] && [ "$latest" != "$current" ]; then
    cat >&2 << EOF
DECIDUOUS: New version available (v${current} -> v${latest})
Run 'deciduous check-update' for details, then 'cargo install deciduous' to upgrade.
After upgrading, run 'deciduous update' to refresh integration files.
Please let the user know a new version is available.
EOF
fi

exit 0
