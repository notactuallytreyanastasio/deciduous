# Rust → Elixir Migration Status

Status of migrating deciduous CLI commands from Rust to Elixir (deciduex).

## Legend

| Status | Meaning |
|--------|---------|
| **Elixir** | Fully implemented in Elixir, Rust delegates to deciduex |
| **Rust** | Still implemented in Rust, not yet migrated |
| **Deprecated** | Will not be migrated, removing from CLI |

## Command Status

### Read Commands (Iteration 0) - COMPLETE

| Command | Status | Notes |
|---------|--------|-------|
| `nodes` | **Elixir** | Lists all decision nodes with filters (`-t`, `-b`) |
| `edges` | **Elixir** | Lists all edges between nodes |
| `show <id>` | **Elixir** | Shows node details, supports `--json` |
| `graph` | **Elixir** | Outputs full graph as JSON |
| `commands` | **Elixir** | Shows recent command log, supports `--limit` |

### Write Commands (Iteration 1-3) - TODO

| Command | Status | Notes |
|---------|--------|-------|
| `add <type> <title>` | Rust | Create new node |
| `link <from> <to>` | Rust | Create edge between nodes |
| `unlink <from> <to>` | Rust | Remove edge |
| `delete <id>` | Rust | Delete node and its edges |
| `status <id> <status>` | Rust | Update node status |
| `prompt <id>` | Rust | Update node prompt |
| `backup` | Rust | Create database backup |

### Document Commands (Iteration 5) - TODO

| Command | Status | Notes |
|---------|--------|-------|
| `doc attach` | Rust | Attach file to node |
| `doc list` | Rust | List attached documents |
| `doc show` | Rust | Show document details |
| `doc describe` | Rust | Set document description |
| `doc open` | Rust | Open document in default app |
| `doc detach` | Rust | Soft-delete document |
| `doc gc` | Rust | Garbage collect orphaned files |

### Server Commands (Iteration 6) - TODO

| Command | Status | Notes |
|---------|--------|-------|
| `serve` | Rust | Start web viewer server |

### Export Commands (Iteration 7) - TODO

| Command | Status | Notes |
|---------|--------|-------|
| `sync` | Rust | Export graph to JSON for GitHub Pages |
| `writeup` | Rust | Generate PR writeup markdown |
| `dot` | Rust | Export graph as DOT format |

### Sync Commands (Iteration 8) - TODO

| Command | Status | Notes |
|---------|--------|-------|
| `diff export` | Rust | Export nodes as shareable patch |
| `diff apply` | Rust | Apply patches from teammates |
| `diff status` | Rust | List available patches |
| `diff validate` | Rust | Validate patch file |

### Advanced Commands (Iteration 9) - TODO

| Command | Status | Notes |
|---------|--------|-------|
| `audit` | Rust | Check graph integrity |
| `pulse` | Rust | Map current design as decisions |
| `narratives` | Rust | Understand system evolution |
| `archaeology` | Rust | Transform narratives to graph |

### Setup Commands - TODO

| Command | Status | Notes |
|---------|--------|-------|
| `init` | Rust | Initialize deciduous in directory |
| `update` | Rust | Update Claude integration files |
| `check-update` | Rust | Check if integration files need updating |
| `hooks` | Rust | Manage git hooks |
| `integration` | Rust | Check integration status |
| `opencode` | Rust | OpenCode integration |
| `completion` | Rust | Generate shell completions |

### Deprecated (Will Not Migrate)

| Command | Status | Notes |
|---------|--------|-------|
| `themes` | Deprecated | Unused feature |
| `tag` | Deprecated | Unused feature |
| `roadmap` | Deprecated | Unused feature |
| `events` | Deprecated | PostgreSQL eliminates need for event sync |
| `migrate` | Deprecated | SQLite-specific migration |

## Testing the Delegation

```bash
# Build Elixir release
cd deciduous_elixir && MIX_ENV=prod mix release deciduex --overwrite

# Set path to Elixir release
export DECIDUEX_PATH="$PWD/_build/prod/rel/deciduex"

# Test delegated commands (these use Elixir)
deciduous nodes
deciduous edges
deciduous show 1
deciduous graph | head
deciduous commands

# Verify fallback (unset DECIDUEX_PATH, uses Rust)
unset DECIDUEX_PATH
deciduous nodes  # Falls back to Rust implementation
```

## Migration Progress

- **Iteration 0**: 5/5 commands (read commands) - COMPLETE
- **Iteration 1**: 0/1 commands (add)
- **Iteration 2**: 0/2 commands (link, unlink)
- **Iteration 3**: 0/4 commands (status, prompt, delete, backup)
- **Iteration 4**: 0/1 commands (backup)
- **Iteration 5**: 0/7 commands (doc subcommands)
- **Iteration 6**: 0/1 commands (serve)
- **Iteration 7**: 0/3 commands (sync, writeup, dot)
- **Iteration 8**: 0/4 commands (diff subcommands)
- **Iteration 9**: 0/4 commands (audit, pulse, narratives, archaeology)

**Total**: 5/32 non-deprecated commands migrated (16%)

## Files

| Component | Location |
|-----------|----------|
| Elixir source | `deciduous_elixir/lib/deciduex/` |
| Elixir tests | `deciduous_elixir/test/` |
| Rust delegation | `src/main.rs` (`find_deciduex_cli()`) |
| Release overlay | `deciduous_elixir/rel/overlays/bin/cli` |
