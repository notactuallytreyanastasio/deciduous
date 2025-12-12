#!/usr/bin/env bash
#
# validate-types.sh - Ensure Rust and TypeScript types stay in sync
#
# This script validates that type definitions match across:
# - src/db.rs (Rust backend)
# - src/tui/types.rs (Rust TUI)
# - web/src/types/graph.ts (TypeScript web)
# - schema/decision-graph.schema.json (canonical schema)
#
# Exit codes:
#   0 - All types are in sync
#   1 - Type mismatch detected
#   2 - Missing file or parse error
#
# Usage:
#   ./scripts/validate-types.sh [--verbose]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

VERBOSE=${1:-}
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

log() {
    echo -e "$1"
}

log_verbose() {
    if [[ "$VERBOSE" == "--verbose" || "$VERBOSE" == "-v" ]]; then
        echo -e "$1"
    fi
}

error() {
    echo -e "${RED}ERROR: $1${NC}" >&2
}

success() {
    echo -e "${GREEN}✓ $1${NC}"
}

warn() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

# File paths
RUST_DB="$PROJECT_ROOT/src/db.rs"
RUST_TUI_TYPES="$PROJECT_ROOT/src/tui/types.rs"
TS_TYPES="$PROJECT_ROOT/web/src/types/graph.ts"
JSON_SCHEMA="$PROJECT_ROOT/schema/decision-graph.schema.json"

# Check all required files exist
check_files() {
    local missing=0
    for file in "$RUST_DB" "$RUST_TUI_TYPES" "$TS_TYPES" "$JSON_SCHEMA"; do
        if [[ ! -f "$file" ]]; then
            error "Missing file: $file"
            missing=1
        fi
    done
    if [[ $missing -eq 1 ]]; then
        exit 2
    fi
    log_verbose "All required files found"
}

# Extract node types from Rust TUI
extract_rust_node_types() {
    # Look for the const array definition, not usages
    grep 'pub const NODE_TYPES' "$RUST_TUI_TYPES" | \
        sed 's/.*\[//' | sed 's/\].*//' | \
        tr ',' '\n' | \
        sed 's/.*"\([^"]*\)".*/\1/' | \
        grep -v '^$' | sort | tr '\n' ' ' | xargs
}

# Extract node types from TypeScript
extract_ts_node_types() {
    grep 'NODE_TYPES' "$TS_TYPES" | head -1 | \
        sed "s/.*\[//" | sed "s/\].*//" | \
        tr ',' '\n' | \
        sed "s/.*'\([^']*\)'.*/\1/" | \
        grep -v '^$' | sort | tr '\n' ' ' | xargs
}

# Extract node types from JSON Schema
extract_schema_node_types() {
    jq -r '.definitions.NodeType.enum[]' "$JSON_SCHEMA" 2>/dev/null | sort | tr '\n' ' ' | xargs
}

# Extract edge types from Rust TUI
extract_rust_edge_types() {
    # Look for the const array definition, not usages
    grep 'pub const EDGE_TYPES' "$RUST_TUI_TYPES" | \
        sed 's/.*\[//' | sed 's/\].*//' | \
        tr ',' '\n' | \
        sed 's/.*"\([^"]*\)".*/\1/' | \
        grep -v '^$' | sort | tr '\n' ' ' | xargs
}

# Extract edge types from TypeScript
extract_ts_edge_types() {
    grep 'EDGE_TYPES' "$TS_TYPES" | head -1 | \
        sed "s/.*\[//" | sed "s/\].*//" | \
        tr ',' '\n' | \
        sed "s/.*'\([^']*\)'.*/\1/" | \
        grep -v '^$' | sort | tr '\n' ' ' | xargs
}

# Extract edge types from JSON Schema
extract_schema_edge_types() {
    jq -r '.definitions.EdgeType.enum[]' "$JSON_SCHEMA" 2>/dev/null | sort | tr '\n' ' ' | xargs
}

# Extract node statuses from Rust TUI
extract_rust_node_statuses() {
    # Look for the const array definition, not usages
    grep 'pub const NODE_STATUSES' "$RUST_TUI_TYPES" | \
        sed 's/.*\[//' | sed 's/\].*//' | \
        tr ',' '\n' | \
        sed 's/.*"\([^"]*\)".*/\1/' | \
        grep -v '^$' | sort | tr '\n' ' ' | xargs
}

# Extract node statuses from TypeScript
extract_ts_node_statuses() {
    grep 'NODE_STATUSES' "$TS_TYPES" | head -1 | \
        sed "s/.*\[//" | sed "s/\].*//" | \
        tr ',' '\n' | \
        sed "s/.*'\([^']*\)'.*/\1/" | \
        grep -v '^$' | sort | tr '\n' ' ' | xargs
}

# Extract node statuses from JSON Schema
extract_schema_node_statuses() {
    jq -r '.definitions.NodeStatus.enum[]' "$JSON_SCHEMA" 2>/dev/null | sort | tr '\n' ' ' | xargs
}

# Check if DecisionNode has change_id field
check_change_id_field() {
    local source=$1
    case $source in
        rust_db)
            grep -q 'pub change_id:' "$RUST_DB" && echo "yes" || echo "no"
            ;;
        typescript)
            grep -q 'change_id:' "$TS_TYPES" && echo "yes" || echo "no"
            ;;
        schema)
            jq -e '.definitions.DecisionNode.properties.change_id' "$JSON_SCHEMA" >/dev/null 2>&1 && echo "yes" || echo "no"
            ;;
    esac
}

# Check if DecisionEdge has change_id fields
check_edge_change_id_fields() {
    local source=$1
    case $source in
        rust_db)
            if grep -q 'pub from_change_id:' "$RUST_DB" && grep -q 'pub to_change_id:' "$RUST_DB"; then
                echo "yes"
            else
                echo "no"
            fi
            ;;
        typescript)
            if grep -q 'from_change_id:' "$TS_TYPES" && grep -q 'to_change_id:' "$TS_TYPES"; then
                echo "yes"
            else
                echo "no"
            fi
            ;;
        schema)
            if jq -e '.definitions.DecisionEdge.properties.from_change_id' "$JSON_SCHEMA" >/dev/null 2>&1 && \
               jq -e '.definitions.DecisionEdge.properties.to_change_id' "$JSON_SCHEMA" >/dev/null 2>&1; then
                echo "yes"
            else
                echo "no"
            fi
            ;;
    esac
}

# Compare types and report differences
compare_types() {
    local name=$1
    local source1_name=$2
    local source2_name=$3
    local source1_types=$4
    local source2_types=$5

    if [[ "$source1_types" == "$source2_types" ]]; then
        log_verbose "  $source1_name == $source2_name"
        return 0
    else
        error "$name mismatch between $source1_name and $source2_name"
        echo "  $source1_name: $source1_types"
        echo "  $source2_name: $source2_types"
        return 1
    fi
}

# Main validation
main() {
    log "Validating type synchronization..."
    log ""

    check_files

    local errors=0

    # === NODE TYPES ===
    log "Checking NODE_TYPES..."
    rust_node_types=$(extract_rust_node_types)
    ts_node_types=$(extract_ts_node_types)
    schema_node_types=$(extract_schema_node_types)

    compare_types "NODE_TYPES" "Rust TUI" "TypeScript" "$rust_node_types" "$ts_node_types" || ((errors++))
    compare_types "NODE_TYPES" "TypeScript" "Schema" "$ts_node_types" "$schema_node_types" || ((errors++))

    if [[ $errors -eq 0 ]]; then
        success "NODE_TYPES in sync: $rust_node_types"
    fi

    # === EDGE TYPES ===
    log ""
    log "Checking EDGE_TYPES..."
    rust_edge_types=$(extract_rust_edge_types)
    ts_edge_types=$(extract_ts_edge_types)
    schema_edge_types=$(extract_schema_edge_types)

    local edge_errors=0
    compare_types "EDGE_TYPES" "Rust TUI" "TypeScript" "$rust_edge_types" "$ts_edge_types" || ((edge_errors++))
    compare_types "EDGE_TYPES" "TypeScript" "Schema" "$ts_edge_types" "$schema_edge_types" || ((edge_errors++))

    if [[ $edge_errors -eq 0 ]]; then
        success "EDGE_TYPES in sync: $rust_edge_types"
    else
        ((errors += edge_errors))
    fi

    # === NODE STATUSES ===
    log ""
    log "Checking NODE_STATUSES..."
    rust_statuses=$(extract_rust_node_statuses)
    ts_statuses=$(extract_ts_node_statuses)
    schema_statuses=$(extract_schema_node_statuses)

    local status_errors=0
    compare_types "NODE_STATUSES" "Rust TUI" "TypeScript" "$rust_statuses" "$ts_statuses" || ((status_errors++))
    compare_types "NODE_STATUSES" "TypeScript" "Schema" "$ts_statuses" "$schema_statuses" || ((status_errors++))

    if [[ $status_errors -eq 0 ]]; then
        success "NODE_STATUSES in sync: $rust_statuses"
    else
        ((errors += status_errors))
    fi

    # === CHANGE_ID FIELD ===
    log ""
    log "Checking change_id field in DecisionNode..."
    rust_change_id=$(check_change_id_field rust_db)
    ts_change_id=$(check_change_id_field typescript)
    schema_change_id=$(check_change_id_field schema)

    if [[ "$rust_change_id" == "yes" && "$ts_change_id" == "yes" && "$schema_change_id" == "yes" ]]; then
        success "change_id field present in all sources"
    else
        error "change_id field missing in one or more sources"
        echo "  Rust DB: $rust_change_id"
        echo "  TypeScript: $ts_change_id"
        echo "  Schema: $schema_change_id"
        ((errors++))
    fi

    # === EDGE CHANGE_ID FIELDS ===
    log ""
    log "Checking from_change_id/to_change_id fields in DecisionEdge..."
    rust_edge_change=$(check_edge_change_id_fields rust_db)
    ts_edge_change=$(check_edge_change_id_fields typescript)
    schema_edge_change=$(check_edge_change_id_fields schema)

    if [[ "$rust_edge_change" == "yes" && "$ts_edge_change" == "yes" && "$schema_edge_change" == "yes" ]]; then
        success "Edge change_id fields present in all sources"
    else
        error "Edge change_id fields missing in one or more sources"
        echo "  Rust DB: $rust_edge_change"
        echo "  TypeScript: $ts_edge_change"
        echo "  Schema: $schema_edge_change"
        ((errors++))
    fi

    # === SUMMARY ===
    log ""
    if [[ $errors -eq 0 ]]; then
        success "All type definitions are in sync!"
        exit 0
    else
        error "Found $errors type synchronization error(s)"
        log ""
        log "To fix:"
        log "  1. Update the source that is out of sync"
        log "  2. Ensure schema/decision-graph.schema.json is the source of truth"
        log "  3. Run this script again to verify"
        exit 1
    fi
}

main "$@"
