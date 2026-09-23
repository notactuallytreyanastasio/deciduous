# Set up the project

Run everything from the repository root. Deciduous finds its data by walking up
from the current directory to the nearest `.deciduous/`, the way git finds
`.git`. A stray `.deciduous/` in a parent directory would capture every write
beneath it, so check for one first:

```sh
git rev-parse --show-toplevel
ls -d .deciduous 2>/dev/null || echo "no .deciduous here"
```

If `.deciduous/` already exists, the project is set up. Skip to
[Upgrading](install.md#upgrading-a-project-that-was-set-up-by-an-older-version)
and do not run `init` again.

## Initialise

```sh
deciduous init            # Claude Code (the default)
deciduous init --opencode # OpenCode: .opencode/ and AGENTS.md
deciduous init --windsurf # Windsurf: .windsurf/
deciduous init --both     # Claude Code and OpenCode
```

Use the flag for the assistant the user actually runs. For Claude Code, `init`
writes:

| Path | What it is | Commit it? |
|------|------------|------------|
| `.deciduous/config.toml` | Branch detection and the `[remote]` server URL and workspace | Yes |
| `CLAUDE.md` | Workflow instructions between `<!-- deciduous:start -->` and `<!-- deciduous:end -->` | Yes |
| `.claude/settings.json`, `.claude/hooks/version-check.sh` | A once-a-day update check, nothing else | Yes |
| `.claude/commands/*.md`, `.claude/skills/*.md` | `/recover`, `/work`, `/decision`, `/pulse`, ... | Yes |
| `.github/workflows/cleanup-decision-graphs.yml` | Removes the PR graph images `dot --auto` makes, after merge | If the team uses `dot --auto` |

Since 1.0.5 `init` writes no GitHub Pages viewer, `docs/` export or Pages
workflow, and no `/sync-graph`. On an older project, `deciduous update` removes
the ones deciduous wrote and leaves `docs/` alone.

## The server step

After writing the files, `init` makes sure the project points at a server that
answers. It fails with exit 1 and the fix if it cannot:

- **`[remote] url` already set:** the server must answer and accept the stored token.
- **No remote, a server already running on this machine:** the project is pointed at it (about a second).
- **No remote, no server:** it downloads the release's `deciduous-mcp-docker.tar.gz` for its own version and checks it against `checksums.txt`. It then starts PostgreSQL and the server on `127.0.0.1:4000` with Docker (a few minutes, the first time on a machine). It stores the token, writes `[remote]`, and registers the server with Claude Code.

```
Shared graph server
   Found local server at http://127.0.0.1:4000
   Wrote .deciduous/config.toml [remote] url = http://127.0.0.1:4000
   Connected http://127.0.0.1:4000 holds 0 nodes, 0 edges in workspace my-project
```

`deciduous update` runs the same check. `DECIDUOUS_NO_SERVER=1` skips it, and
exists only for tests and CI in throwaway directories. Do not set it to get
past a failure; fix what the message names.

Show the user the list of new files before you commit any of them. Commit
explicitly by path, never with `git add -A` or `git add .`:

```sh
git add .deciduous/config.toml CLAUDE.md .claude/
git commit -m "chore: set up deciduous"
```

If the project already had a `CLAUDE.md`, `init` adds its section between the
markers and leaves the rest alone. Keep your own instructions outside the
markers. `deciduous update` rewrites everything between them.

## Environment

| Variable | Meaning |
|----------|---------|
| `DECIDUOUS_MCP_TOKEN` | Bearer token for the shared server. Checked before `~/.config/deciduous/credentials` |

Next: [connect to the shared server](shared-graph.md).
