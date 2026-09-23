# Verify

Run these after setting up and report each result to the user as pass or
fail, with the output. Do not report the setup as done until every line that
applies has passed. A setup that looks right but writes to the wrong place is
worse than one that fails loudly.

## Every project

```sh
deciduous --version                 # 1.0.5 or newer
deciduous check-update; echo $?     # 0: integration files match the binary
git ls-files --error-unmatch .deciduous/config.toml
```

`git ls-files` fails until the files are committed. Report that as a step
left to do, not as a failure.

## Shared server

```sh
deciduous remote status
```

Expected:

```
Remote: https://example.com/deciduous-mcp
Workspace: my-project

                 local    remote
  nodes            ...      ...

OK: counts match.
```

Then prove that your MCP writes land where the user expects. This is the check
that catches a wrong workspace:

1. Call `add_node` with `node_type: "observation"`, a title such as "deciduous
   setup verified from <client>", `workspace`, `branch`, and `parent_id` set
   to the goal you are working under (or no parent if there is none yet).
2. Call `query_nodes` with the same `workspace` and `branch`, and a `search`
   for that title. It must come back.
3. Run `deciduous remote status`. The remote count went up by one. The local
   count did not, which is correct: MCP writes go to the server.
4. Optionally `deciduous remote pull`, and the counts match again.

Leave the node in place. It records when and from where the setup was verified.
