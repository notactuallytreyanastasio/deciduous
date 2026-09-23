# Install

You need one binary, `deciduous`: the CLI that sets up a project, points it at
the shared server and installs the agent instructions. The shared server is a
separate program. `deciduous init` sets one up on this machine with Docker when
the project has no remote (see [Set up the project](project.md#the-server-step)),
so Docker must be installed and running unless the project uses a team's server.

## Check what is already there

```sh
which -a deciduous
deciduous --version
```

`which -a` lists every copy on `PATH`. Machines often have two, one from
cargo and one from Homebrew, and the first one wins. If they report different
versions, tell the user which one runs, and fix `PATH` or remove the stale copy
rather than installing a third.

Use **1.0.5 or newer**. 1.0.2 installed hooks that deny an agent's tool calls
until it writes to the graph. 1.0.3 removed them, 1.0.4 moved the logging
guidance into the server's `initialize` reply, and 1.0.5 made `init` set up or
check the server and dropped GitHub Pages. `deciduous update` on 1.0.5 brings
an older project up to date.

## Install or upgrade

Pick the channel the machine already uses. Otherwise use Homebrew on macOS and
cargo when a Rust toolchain is present.

```sh
# Homebrew (macOS, Linux)
brew install notactuallytreyanastasio/tap/deciduous
brew upgrade deciduous

# crates.io (needs a Rust toolchain)
cargo install deciduous --locked

# A release binary, no toolchain needed
# https://github.com/notactuallytreyanastasio/deciduous/releases/latest
#   deciduous-darwin-arm64   deciduous-darwin-amd64
#   deciduous-linux-arm64    deciduous-linux-amd64
#   deciduous-windows-amd64.exe
#   checksums.txt            (verify before running)
```

Then confirm:

```sh
deciduous --version
# deciduous 1.0.5
```

## Upgrading a project that was set up by an older version

The binary and each project's integration files are versioned separately.
After upgrading the binary, run this in each project:

```sh
deciduous check-update   # exit 0: files current; exit 1: run update
deciduous update
```

`deciduous update` replaces only files deciduous wrote
(`.claude/commands/`, `.claude/skills/`, the section of `CLAUDE.md` between
`<!-- deciduous:start -->` and `<!-- deciduous:end -->`). A file the user
changed is kept; Markdown gets the new template appended in a marked block.
Every file it changes is copied to `.deciduous/update-backups/<time>/` first.
It removes the logging hook scripts it once installed and their entries in
`.claude/settings.json`, and keeps any hook script the user wrote. It removes
`/sync-graph` and `.github/workflows/deploy-pages.yml` when deciduous wrote them,
and leaves `docs/` alone. In a project with a `[remote]` it leaves `.gitignore`
and `.gitattributes` alone. It then checks the project's server, the same way
`init` does. `deciduous update --all ~/code` does every project under a directory.
Show the user `git diff` afterwards. The changes are theirs to commit.
