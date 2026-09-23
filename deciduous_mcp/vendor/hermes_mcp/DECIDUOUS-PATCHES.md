# hermes_mcp 0.14.1, vendored with four patches

This is the hex package `hermes_mcp` 0.14.1 as fetched, with four changes
deciduous needs and upstream does not have. It is a `path:` dependency in
`mix.exs` so `mix deps.get` never overwrites it. Upgrading Hermes means
re-applying these four hunks or confirming upstream made them unnecessary.

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
(set in `DeciduousMcp.Application`) is the transport's budget, and it still
replies `{:error, :server_unavailable}` when it expires. It only stops
waiting; patch 3 is what stops the handler.

## 3. A handler that outlives `request_deadline` is killed and answered

`lib/hermes/server/base.ex`, `lib/hermes/server/supervisor.ex`. New option
`:request_deadline` (ms, default none, which is upstream's behaviour),
passed from the server's child spec through the supervisor to Base.
`start_request_task/4` arms a timer next to each handler task. If it fires
while the task is still pending, Base kills the task with
`Task.Supervisor.terminate_child/2`, replies under the request's own id with
`"<tool> did not finish within <n> and was stopped; nothing it had not
committed was kept"`, and logs `request_deadline_exceeded`.

The transport's `request_timeout` cannot do this job. When it fires the
client is told to go away while Base keeps running the handler for nobody,
and a client that retries a write gets it twice. Killing the task is what
makes the deadline safe for writes: a transaction whose owner dies is rolled
back.

deciduous sets 60 s, under Cloudflare's 100 s origin cut and Claude Code's
120 s move-to-background. Test: `test/deciduous_mcp/mcp/request_deadline_test.exs`.

## 4. `Hermes.Logging.should_log?/1` compared levels backwards

`lib/hermes/logging.ex`. Upstream was
`Logger.compare_levels(config_level, level) != :lt`, which passes only
messages at or below the configured level. At `:info` in production every
`:warning` and `:error` Hermes emits (`request_handler_crashed`,
`server_call_failed`, `request_error`) was dropped before reaching Logger:

    config=info msg=error compare(config,msg)=lt -> hermes logs? false
    config=info msg=debug compare(config,msg)=gt -> hermes logs? true

The arguments are swapped. The deadline test asserts the log line, and was
the first thing to notice it was missing.

Patch files with the full rationale and measurements for 1 and 2:
`serialization-hermes-base-async.patch` and
`wheretime-02-hermes-transport-call-infinity.patch` in the 2026-09-22 hangs
investigation.
