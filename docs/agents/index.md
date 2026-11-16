# Deciduous, for agents

This section is written for you, the coding agent, not for the person you work
for. It tells you how to install deciduous, wire it into the project you are
in, connect to a shared graph if there is one, and keep the graph current while
you work. These pages describe deciduous 1.0.6, and the output shown is what
it printed. Where an older version behaves differently, the page says so.

The pages for people are at [the main site](../index.html). They explain what the
graph is for. You do not need them to set it up.

## What you are setting up

Deciduous records the reasoning behind a project as a graph: what was asked
(`goal`), the approaches considered (`option`), the one chosen (`decision`),
the work done (`action`), what happened (`outcome`), and anything learned on
the way (`observation`, `revisit`). The next session, yours or another agent's,
reads that graph instead of guessing why the code looks the way it does.

The graph lives on a shared server: one PostgreSQL database behind an HTTP MCP
endpoint, one workspace per repository. You write to it with the MCP tools
(`add_node`, `add_edge`, ...), and every other agent on the workspace sees the
write as it lands.

The one fact that breaks most setups: **the `deciduous` CLI does not write to
the server.** `deciduous add` after `deciduous remote init` still does not reach
it, and `deciduous remote status` shows the gap. Log through MCP only. See
[Shared graph](shared-graph.md).

## Do it in this order

1. [Install](install.md) the `deciduous` binary, 1.0.6 or newer, and check it. Docker must be running unless the project uses someone else's server.
2. [Set up the project](project.md): `deciduous init`. It will not finish until the project points at a server that answers. With no remote configured it asks where the graph lives (the user answers), and without a terminal it stops and names `remote setup --local` / `--url`.
3. To use a team's server instead, [connect to it](shared-graph.md) before `init`, or run `deciduous remote setup`.
4. Read [Logging while you work](logging.md). Nothing enforces it; the graph is only as good as what you write.
5. Run the checks in [Verify](verify.md) and report each result to the user.
6. When something fails, look in [Troubleshooting](troubleshooting.md) before improvising.

[MCP tools](tools.md) lists every tool with its required arguments.

## Before you start, ask the user two things

- Should the graph live on a server on this machine (`init` sets one up with
  Docker), or on a team's server? For a team's server: what is its URL, and is
  the token already on this machine (`~/.config/deciduous/credentials`, or
  `DECIDUOUS_MCP_TOKEN` in the environment)? Never ask them to paste the token
  into the chat. Have them run `deciduous remote login` or
  `deciduous remote setup` themselves.
- Should the integration files (`.claude/`, `CLAUDE.md`) be committed
  for the whole team, or kept to this checkout?

Do not guess either answer. The first one decides where every node lands.

## Machine-readable entry points

- [`/llms.txt`](../llms.txt): index of these pages, with one line per page.
- [`/agents/llms-full.txt`](llms-full.txt): every page in this section, concatenated.
- Every page here is plain Markdown. Fetch the `.md`.
