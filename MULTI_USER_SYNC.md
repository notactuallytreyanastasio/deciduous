# Multi-User Graph Sync Design

## Problem Statement

Multiple users work on the same codebase, each with their own local `.deciduous/deciduous.db` (gitignored). They need to:

1. Export their new decisions as "diffs" or "patches"
2. Commit these patches to the shared repository
3. Apply patches from other users to their local database
4. Handle references to existing nodes (edges pointing to nodes from previous sessions)
5. Avoid ID conflicts when multiple users create nodes concurrently

## Design: jj-Inspired Dual-ID Model

Inspired by [Jujutsu's](https://github.com/jj-vcs/jj) separation of "change IDs" (stable) vs "commit IDs" (content-addressable):

### Core Concept

Each node has two identifiers:
- **`id`** (integer): Local database primary key, auto-incremented, different per machine
- **`change_id`** (UUID): Globally unique, stable, same across all databases

### Edges Reference `change_id`

When syncing, edges use `change_id` to reference nodes, not integer `id`. This allows:
- Node 5 on Alice's machine to reference Node 3 from the shared graph
- The same logical reference works on Bob's machine where those nodes might have different local IDs

## Diff/Patch Format

```json
{
  "version": "1.0",
  "author": "alice",
  "branch": "feature/auth",
  "created_at": "2025-12-10T12:00:00Z",
  "base_commit": "abc123",
  "nodes": [
    {
      "change_id": "550e8400-e29b-41d4-a716-446655440000",
      "node_type": "goal",
      "title": "Implement user authentication",
      "description": "...",
      "status": "active",
      "metadata_json": "{\"confidence\": 85, \"branch\": \"feature/auth\"}"
    }
  ],
  "edges": [
    {
      "from_change_id": "550e8400-e29b-41d4-a716-446655440000",
      "to_change_id": "existing-node-change-id",
      "edge_type": "leads_to",
      "rationale": "New goal builds on existing decision"
    }
  ]
}
```

## Workflow

### Export a Diff

```bash
# Export all nodes created since last sync
deciduous diff export --since-commit abc123 -o patches/alice-auth.json

# Export specific nodes
deciduous diff export --nodes 172-180 -o patches/alice-auth.json

# Export nodes from current branch only
deciduous diff export --branch feature/auth -o patches/alice-auth.json
```

### Apply a Diff

```bash
# Apply a patch file to local database
deciduous diff apply patches/bob-refactor.json

# Dry-run to see what would change
deciduous diff apply --dry-run patches/bob-refactor.json

# Apply all patches in directory
deciduous diff apply patches/*.json
```

### PR Workflow

1. Alice works on `feature/auth`, creates nodes 172-180
2. Alice exports: `deciduous diff export --branch feature/auth -o .deciduous/patches/alice-auth.json`
3. Alice commits the patch file (not the database)
4. Alice opens PR with the patch file
5. PR is reviewed, merged to main
6. Bob pulls main, runs: `deciduous diff apply .deciduous/patches/alice-auth.json`
7. Bob's local database now has Alice's nodes (with potentially different local IDs but same change_ids)

## Schema Migration

Add `change_id` column to `decision_nodes` and reference columns to `decision_edges`:

```sql
-- Migration: Add change_id to nodes
ALTER TABLE decision_nodes ADD COLUMN change_id TEXT;
UPDATE decision_nodes SET change_id = lower(hex(randomblob(16))) WHERE change_id IS NULL;
CREATE UNIQUE INDEX idx_nodes_change_id ON decision_nodes(change_id);

-- Migration: Add change_id references to edges
ALTER TABLE decision_edges ADD COLUMN from_change_id TEXT;
ALTER TABLE decision_edges ADD COLUMN to_change_id TEXT;
-- Backfill from existing integer references
UPDATE decision_edges SET
  from_change_id = (SELECT change_id FROM decision_nodes WHERE id = decision_edges.from_node_id),
  to_change_id = (SELECT change_id FROM decision_nodes WHERE id = decision_edges.to_node_id);
```

## Conflict Resolution

### Node Conflicts
- Nodes are identified by `change_id` - if the same `change_id` exists, skip (idempotent)
- Different nodes (different `change_id`) never conflict even if they have same title

### Edge Conflicts
- Edges are identified by (from_change_id, to_change_id, edge_type) tuple
- Duplicate edges are skipped (idempotent)

### Merge Strategy
Patches are additive by default. No deletion through patches (yet).

## Future Enhancements

1. **Tombstones**: Mark deleted nodes/edges in patches
2. **Branch Subscriptions**: Auto-apply patches from watched branches
3. **Conflict Detection**: Warn when edges reference non-existent change_ids
4. **Compression**: Binary patch format for large graphs
5. **Signed Patches**: Cryptographic signatures for audit trail

## Implementation Phases

### Phase 1: Schema Migration (This PR)
- Add `change_id` column to nodes
- Add `from_change_id`/`to_change_id` to edges
- Migrate existing data to have change_ids
- Update node creation to generate UUIDs

### Phase 2: Export Command
- `deciduous diff export` command
- JSON patch file format
- Filter by commit, branch, or node range

### Phase 3: Apply Command
- `deciduous diff apply` command
- Idempotent application
- Dry-run mode

### Phase 4: PR Integration
- `.deciduous/patches/` directory convention
- Auto-detection of unapplied patches
- `deciduous diff status` command
