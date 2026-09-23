# hermes_mcp 0.14.1, vendored with two patches

This is the hex package `hermes_mcp` 0.14.1 as fetched, with two changes
deciduous needs and upstream does not have. It is a `path:` dependency in
`mix.exs` so `mix deps.get` never overwrites it. Upgrading Hermes means
re-applying these two hunks or confirming upstream made them unnecessary.

## 1. Request handlers run in a Task, not in `Hermes.Server.Base`

`lib/hermes/server/base.ex`. Base is one GenServer per server module and
every session's requests pass through it. Upstream runs
`module.handle_request/2` inline in `handle_call({:request, ...})`, so one
slow tool call (a 33 MB `get_graph`) blocked every other session's call, and
a handler exception took Base down and, through the `one_for_all`
supervisor, every session with it.

Now the generic request clause returns `{:async, request, state}`; Base
starts the handler under the Task.Supervisor Hermes already runs, parks the
caller's `from` in `state.pending_requests`, and replies from `handle_info`
when the task reports back. The task also encodes the JSON-RPC envelope,
because `Message.encode_response` on a 33 MB result is 400-640 ms of
re-escaping and would otherwise be the last serial section. A task crash
becomes a JSON-RPC error under the request's id. `initialize`, `ping`,
`logging/setLevel`, notifications and responses still run inline.

Measured locally on the same data (epstein, 7,805 nodes), before -> after:

    check_activity issued 1s into another session's get_graph   0.75-0.88s -> 0.04s
    8 check_activity during a get_graph                         1.38-1.40s -> 0.05s
    8 get_graph at once                                  4 of 8 fail at 5.06s -> all 200, 3.4-3.9s

## 2. The plug's call into the transport has no 5 s ceiling

`lib/hermes/server/transport/streamable_http.ex`. `handle_message/4` and
`handle_message_for_sse/4` used `GenServer.call`'s default 5_000 ms. The
transport answers from `handle_info` when its task hears back from Base, so
the plug process sat in that call for the whole request and exited at 5 s:
Bandit answered an empty HTTP 500 while Base kept running the abandoned call.
Both calls now pass `:infinity`; the transport's own `request_timeout`
(set to 4 minutes in `DeciduousMcp.Application`, under Claude Code's 300 s
abort) is the one budget, and it still replies `{:error, :server_unavailable}`
when it expires.

Patch files with the full rationale and measurements:
`serialization-hermes-base-async.patch` and
`wheretime-02-hermes-transport-call-infinity.patch` in the 2026-09-22 hangs
investigation.
