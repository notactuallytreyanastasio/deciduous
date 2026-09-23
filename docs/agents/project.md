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
| `docs/index.html`, `docs/graph-data.json` | A static graph viewer for GitHub Pages | Only if the user wants the graph published |
| `.github/workflows/deploy-pages.yml`, `cleanup-decision-graphs.yml` | Pages deploy and PNG cleanup | Only with the viewer |

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
