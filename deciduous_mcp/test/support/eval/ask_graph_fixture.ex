defmodule DeciduousMcp.Eval.AskGraphFixture do
  @moduledoc """
  The fixture decision graph and question set for the ask_graph retrieval
  eval (`DeciduousMcp.Eval.AskGraph`).

  The graph is a small but realistic project history: six goals, each with
  options, a decision that chose one and rejected others, actions carrying
  files and commits, outcomes with numbers, observations, and two pivots
  where a revisit supersedes an earlier decision. Distractor nodes share
  vocabulary with the real answers ("rate limit", "session", "token",
  "Postgres", "event log", "layout") without being about the same thing.

  Edges follow the shapes the tools write: goal -> option and goal ->
  decision (leads_to), decision -> option (chosen / rejected, as
  log_decision writes them),
  decision -> action -> outcome, observation -> revisit -> new decision.

  The questions were written from what a user would ask of this history,
  not from what the current ask_graph matches. Each lists `expect`: the
  node keys a good answer must contain. Categories mirror LoCoMo's split as
  it applies to a decision graph:

    * `:single_hop` -- one node answers it
    * `:multi_hop`  -- the answer spans linked nodes (a choice and the
      option it beat, an action and its outcome)
    * `:temporal`   -- pivots: what replaced what, what was tried first
    * `:file`       -- asked by file path or commit
    * `:adversarial` -- about something never decided. `expect` is empty;
      `absent_terms` are the words the graph has nothing about. A good
      answer returns nothing, or says those terms matched nothing and
      returns at most three results (see `DeciduousMcp.Eval.AskGraph`).

  ## The held-out set

  `questions/0` was written alongside the retrieval routes, and the routes'
  cue lists repeat its wording ("why", "replaced", "decide", "happened",
  "touched"). A score on it partly measures that overlap. `held_out/0` is
  a second set over the same graph, written afterwards, phrased as a user
  would ask without any word that switches a route on. It is scored and
  reported separately and must not be used to tune routes.

  Words avoided, and checked by `validate!/0` with the same tokenisation
  Retrieval uses: every cue of `Retrieval.default_routes/0`

      rationale:  why reason reasons rationale decide decided decision
                  decisions chose choose chosen pick picked rejected reject
                  instead tradeoff tradeoffs motivation because problem
                  cause caused
      history:    history pivot pivoted pivots revisit revisited reconsider
                  reconsidered superseded supersede changed evolve evolved
                  evolution originally trace lineage replace replaced
                  reverse reversed reversal undo undid earlier previous
                  previously
      dependency: blocked blocking blocks blocker blockers depends depend
                  dependency dependencies requires require required
                  prerequisite prerequisites waiting stuck enables
      outcome:    outcome outcomes result results fix fixed work worked
                  perform performed performance happened succeed succeeded
                  fail failed effect

  and every cue of `Related.route/0` (file files path paths module commit
  sha touched changed). Paths themselves are allowed: naming a path is
  how a user asks about one, and it is not a cue word.
  """

  @doc "Nodes as `{key, node_type, title, attrs}`, in creation order."
  def nodes do
    [
      # --- G1: multi-user sync, with a pivot ---------------------------------
      {:g_sync, "goal", "Multi-user sync for the decision graph",
       %{
         description:
           "Two people working on the same repo need to see each other's nodes after a git pull.",
         status: "active"
       }},
      {:o_eventlog, "option", "JSONL event log of patches replayed on pull",
       %{description: "Append every mutation to .deciduous/events.jsonl and replay it on import."}},
      {:o_graphjson, "option", "One graph.json file merged record by record",
       %{
         description:
           "Commit a single .deciduous/graph.json and merge concurrent edits with a git merge driver."
       }},
      {:o_central_pg, "option", "Central Postgres server that every client writes to",
       %{
         description: "Hosted database; clients write over the network instead of to local files."
       }},
      {:d_eventlog, "decision", "Sync through a JSONL event log",
       %{
         description: "Chose the append-only event log: it works offline and needs no server.",
         status: "superseded"
       }},
      {:a_eventlog, "action", "Implement event log export and import",
       %{files: ["src/events.rs", "src/main.rs"], commit: "a1b2c3d", status: "completed"}},
      {:oc_eventlog, "outcome", "Event log replay duplicated nodes after a rebase",
       %{
         description:
           "After rebasing a branch, import replayed 212 patches a second time and created duplicate nodes.",
         status: "completed"
       }},
      {:ob_replay, "observation", "Replaying the log after a rebase applies the same patch twice",
       %{
         description:
           "Patch identity is the file offset, which a rebase changes, so already-applied patches look new."
       }},
      {:r_sync, "revisit", "Reconsidering the sync format after duplicate replays",
       %{description: "The event log cannot tell an applied patch from a new one."}},
      {:d_graphjson, "decision", "Sync through one graph.json merged by a git merge driver",
       %{
         description:
           "Records are keyed by change_id; the merge driver merges field by field, newer updated_at wins.",
         status: "active"
       }},
      {:a_mergedriver, "action", "Write the git merge driver for graph.json",
       %{files: ["src/records.rs", ".gitattributes"], commit: "9f3e2a1", status: "completed"}},
      {:oc_mergedriver, "outcome", "Concurrent edits merge cleanly in the 40-branch test",
       %{
         description: "40 branches editing the same graph merged with zero conflict markers.",
         status: "completed"
       }},

      # --- G2: API rate limiting ---------------------------------------------
      {:g_ratelimit, "goal", "Rate limiting for the public API",
       %{description: "Stop one client from exhausting the API for everyone.", status: "active"}},
      {:o_bucket, "option", "Token bucket per API key in Redis",
       %{
         description:
           "Each API key gets a bucket refilled at a fixed rate; Redis holds the counters."
       }},
      {:o_fixedwindow, "option", "Fixed window counter in Postgres",
       %{description: "Count requests per key per minute in a Postgres table."}},
      {:o_nginx_ip, "option", "Rate limit at nginx by client IP address",
       %{description: "limit_req keyed on the client address, no application code."}},
      {:d_bucket, "decision", "Use a token bucket per API key",
       %{
         description:
           "Token bucket allows short bursts and keys on the caller's identity rather than its address.",
         status: "active"
       }},
      {:ob_nat, "observation", "Mobile carriers put thousands of users behind one NAT address",
       %{description: "Per-IP limits would throttle every user on the same carrier gateway."}},
      {:a_limiter, "action", "Add token bucket middleware to the API pipeline",
       %{
         files: ["lib/api/rate_limiter.ex", "lib/api/router.ex"],
         commit: "4c7d8e0",
         status: "completed"
       }},
      {:oc_limiter, "outcome", "Rate limiter adds 0.4ms at p99",
       %{
         description: "Measured on staging over 1M requests: p50 0.1ms, p99 0.4ms.",
         status: "completed"
       }},

      # --- G3: viewer authentication, with a pivot ---------------------------
      {:g_auth, "goal", "Authentication for the web viewer",
       %{
         description: "Only people on the team should be able to open the viewer.",
         status: "active"
       }},
      {:o_cookies, "option", "Session cookies with a server-side session store",
       %{description: "An opaque cookie id; the session lives in the database."}},
      {:o_jwt, "option", "JWT access tokens with refresh token rotation",
       %{description: "Stateless signed tokens, short-lived, rotated with a refresh token."}},
      {:d_jwt, "decision", "Use JWT with refresh rotation for viewer login",
       %{description: "Stateless, so the viewer needs no session table.", status: "superseded"}},
      {:ob_jwt_size, "observation", "JWT payload too large for mobile clients",
       %{
         description:
           "Full claims push the header past 8KB; the refresh round trip adds 2-3s on slow connections."
       }},
      {:r_auth, "revisit", "Reconsidering the token strategy for the viewer",
       %{description: "Stateless tokens cost more than the session table they avoided."}},
      {:d_cookies, "decision", "Opaque session cookies backed by a Postgres session table",
       %{status: "active"}},
      {:a_sessionplug, "action", "Replace the JWT middleware with a session plug",
       %{files: ["lib/web/session_guard.ex"], commit: "e5f6a7b", status: "completed"}},
      {:oc_login, "outcome", "Login round trip down from 2.8s to 0.3s on 3G",
       %{status: "completed"}},

      # --- G4: local storage ---------------------------------------------------
      {:g_storage, "goal", "Storage engine for the local CLI",
       %{description: "Where the CLI keeps the decision graph on each machine.", status: "active"}},
      {:o_sqlite, "option", "SQLite through Diesel",
       %{description: "One file, no server process."}},
      {:o_pg_local, "option", "A local Postgres server",
       %{description: "Full SQL, but every user must install and run a database."}},
      {:d_sqlite, "decision", "Store the local graph in SQLite through Diesel",
       %{files: ["src/db.rs"], status: "active"}},
      {:a_changeid, "action", "Add the change_id column migration",
       %{
         files: ["src/db.rs", "migrations/2025-11-02-add-change-id/up.sql"],
         commit: "b8c9d0e",
         status: "completed"
       }},
      {:ob_busy, "observation", "Concurrent CLI and serve processes hit SQLITE_BUSY",
       %{
         description: "The rollback journal locks the whole file while serve holds a read.",
         files: ["src/db.rs"]
       }},
      {:a_wal, "action", "Enable WAL journal mode when opening the database",
       %{files: ["src/db.rs"], commit: "d1e2f3a", status: "completed"}},
      {:oc_wal, "outcome", "SQLITE_BUSY errors gone in the parallel test run",
       %{
         description: "0 failures in 500 runs of the concurrent test, down from 37.",
         status: "completed"
       }},

      # --- G5: viewer layout ---------------------------------------------------
      {:g_layout, "goal", "Readable graph layout in the web viewer", %{status: "active"}},
      {:o_dagre, "option", "Dagre layered layout",
       %{description: "Top-down ranks follow edge direction."}},
      {:o_force, "option", "Force-directed D3 layout",
       %{description: "Physics simulation positions nodes."}},
      {:d_dagre, "decision", "Lay out the viewer graph with Dagre",
       %{
         description:
           "Decision flow reads top to bottom; ranks make goal -> outcome chains legible.",
         status: "active"
       }},
      {:a_chains, "action", "Build chains with BFS over connected components",
       %{files: ["web/src/utils/graphProcessing.ts"], commit: "7a8b9c0", status: "completed"}},
      {:oc_render, "outcome", "Viewer renders a 1,500-node graph in 900ms",
       %{status: "completed"}},

      # --- G6: release pipeline -----------------------------------------------
      {:g_release, "goal", "Automated releases", %{status: "active"}},
      {:o_tag_ci, "option", "Tag-triggered GitHub Actions release workflow", %{}},
      {:o_manual, "option", "Publish by hand from a laptop with cargo publish", %{}},
      {:d_tag_ci, "decision", "Release from a tag-triggered workflow", %{status: "active"}},
      {:ob_bot_tags, "observation",
       "Tags pushed by the bot token never trigger the release workflow",
       %{
         description:
           "GITHUB_TOKEN pushes do not start other workflows; the release has to be dispatched."
       }},
      {:a_dispatch, "action", "Add a workflow_dispatch trigger to the release workflow",
       %{files: [".github/workflows/release.yml"], commit: "c3d4e5f", status: "completed"}},

      # --- Distractors: shared vocabulary, different subject ------------------
      {:x_dockerhub, "goal", "Stay under the Docker Hub pull rate limit in CI",
       %{
         description: "Anonymous pulls are limited to 100 per six hours per IP.",
         status: "pending"
       }},
      {:x_gh_ratelimit, "observation", "GitHub API rate limit hit by the changelog script",
       %{files: ["scripts/changelog.sh"]}},
      {:x_session_var, "action", "Rename the session variable in the release shell script",
       %{files: ["scripts/release.sh"], status: "completed"}},
      {:x_pg_testdb, "observation", "Postgres test database name collides between worktrees",
       %{description: "Two worktrees running mix test share deciduous_mcp_test."}},
      {:x_token_count, "goal", "Warn when a prompt's token count gets large",
       %{status: "pending"}},
      {:x_layout_cache, "option", "Cache node positions in localStorage between visits",
       %{description: "Skip the layout pass when the graph has not changed."}},
      {:x_listener, "outcome", "Event listener reconnects after a Postgres restart",
       %{description: "The LISTEN connection re-subscribes within 2s.", status: "completed"}}
    ]
  end

  @doc "Edges as `{from_key, to_key, edge_type, rationale}`."
  def edges do
    [
      # G1
      {:g_sync, :o_eventlog, "leads_to", nil},
      {:g_sync, :o_graphjson, "leads_to", nil},
      {:g_sync, :o_central_pg, "leads_to", nil},
      {:g_sync, :d_eventlog, "leads_to", nil},
      {:d_eventlog, :o_eventlog, "chosen", "Works offline and needs no server"},
      {:d_eventlog, :o_central_pg, "rejected",
       "Needs a hosted server and network access for every write; users work offline"},
      {:d_eventlog, :a_eventlog, "leads_to", nil},
      {:a_eventlog, :oc_eventlog, "leads_to", nil},
      {:oc_eventlog, :ob_replay, "leads_to", nil},
      {:d_eventlog, :r_sync, "leads_to", "Superseded"},
      {:ob_replay, :r_sync, "leads_to", "Forced rethinking of the format"},
      {:r_sync, :d_graphjson, "leads_to", "Replacement"},
      {:d_graphjson, :o_graphjson, "chosen",
       "Records merge by change_id; a rebase cannot duplicate them"},
      {:d_graphjson, :o_eventlog, "rejected", "Replays applied patches twice after a rebase"},
      {:d_graphjson, :a_mergedriver, "leads_to", nil},
      {:a_mergedriver, :oc_mergedriver, "leads_to", nil},
      # G2
      {:g_ratelimit, :o_bucket, "leads_to", nil},
      {:g_ratelimit, :o_fixedwindow, "leads_to", nil},
      {:g_ratelimit, :o_nginx_ip, "leads_to", nil},
      {:g_ratelimit, :d_bucket, "leads_to", nil},
      {:d_bucket, :o_bucket, "chosen", "Allows bursts, keyed on the API key"},
      {:d_bucket, :o_fixedwindow, "rejected",
       "One write per request to Postgres, and a burst at the window edge doubles the allowed rate"},
      {:d_bucket, :o_nginx_ip, "rejected", "Carrier NAT puts many users behind one address"},
      {:ob_nat, :d_bucket, "leads_to", "Why per-IP limiting was rejected"},
      {:d_bucket, :a_limiter, "leads_to", nil},
      {:a_limiter, :oc_limiter, "leads_to", nil},
      # G3
      {:g_auth, :o_cookies, "leads_to", nil},
      {:g_auth, :o_jwt, "leads_to", nil},
      {:g_auth, :d_jwt, "leads_to", nil},
      {:d_jwt, :o_jwt, "chosen", "Stateless"},
      {:d_jwt, :o_cookies, "rejected",
       "Needs a session table and a database read on every request"},
      {:d_jwt, :ob_jwt_size, "leads_to", nil},
      {:ob_jwt_size, :r_auth, "leads_to", "Forced rethinking"},
      {:d_jwt, :r_auth, "leads_to", "Superseded"},
      {:r_auth, :d_cookies, "leads_to", "Replacement"},
      {:d_cookies, :o_cookies, "chosen", "Small cookie, one indexed read per request"},
      {:d_cookies, :o_jwt, "rejected", "Payload size and refresh latency on mobile"},
      {:d_cookies, :a_sessionplug, "leads_to", nil},
      {:a_sessionplug, :oc_login, "leads_to", nil},
      # G4
      {:g_storage, :o_sqlite, "leads_to", nil},
      {:g_storage, :o_pg_local, "leads_to", nil},
      {:g_storage, :d_sqlite, "leads_to", nil},
      {:d_sqlite, :o_sqlite, "chosen", "Nothing to install"},
      {:d_sqlite, :o_pg_local, "rejected", "Every user would have to run a database server"},
      {:d_sqlite, :a_changeid, "leads_to", nil},
      {:d_sqlite, :ob_busy, "leads_to", nil},
      {:ob_busy, :a_wal, "leads_to", nil},
      {:a_wal, :oc_wal, "leads_to", nil},
      # G5
      {:g_layout, :o_dagre, "leads_to", nil},
      {:g_layout, :o_force, "leads_to", nil},
      {:g_layout, :x_layout_cache, "leads_to", nil},
      {:g_layout, :d_dagre, "leads_to", nil},
      {:d_dagre, :o_dagre, "chosen", "Ranks follow the decision flow"},
      {:d_dagre, :o_force, "rejected", "Nodes jitter and overlap past 200 nodes"},
      {:d_dagre, :a_chains, "leads_to", nil},
      {:a_chains, :oc_render, "leads_to", nil},
      # G6
      {:g_release, :o_tag_ci, "leads_to", nil},
      {:g_release, :o_manual, "leads_to", nil},
      {:g_release, :d_tag_ci, "leads_to", nil},
      {:d_tag_ci, :o_tag_ci, "chosen", "Reproducible builds for every target"},
      {:d_tag_ci, :o_manual, "rejected", "Depends on one laptop's toolchain"},
      {:d_tag_ci, :ob_bot_tags, "leads_to", nil},
      {:ob_bot_tags, :a_dispatch, "leads_to", nil}
    ]
  end

  @doc """
  Questions as maps: `id`, `category`, `question`, `expect` (node keys a good
  answer must contain) and, for adversarial ones, `absent_terms`.
  """
  def questions do
    [
      # --- single-hop -----------------------------------------------------------
      q(:s01, :single_hop, "What algorithm do we use for API rate limiting?", [:d_bucket]),
      q(:s02, :single_hop, "How does the viewer lay out the graph?", [:d_dagre]),
      q(:s03, :single_hop, "What does the CLI use to store the graph locally?", [:d_sqlite]),
      q(:s04, :single_hop, "How do we publish releases?", [:d_tag_ci]),
      q(:s05, :single_hop, "How much latency does the rate limiter add?", [:oc_limiter]),
      q(:s06, :single_hop, "How long does the viewer take to render a big graph?", [:oc_render]),
      q(:s07, :single_hop, "Why were we getting SQLITE_BUSY errors?", [:ob_busy]),
      q(:s08, :single_hop, "Which approaches did we consider for multi-user sync?", [
        :o_eventlog,
        :o_graphjson,
        :o_central_pg
      ]),
      q(:s09, :single_hop, "What are we trying to achieve with authentication?", [:g_auth]),
      q(:s10, :single_hop, "Do tags pushed by the bot start the release workflow?", [:ob_bot_tags]),

      # --- multi-hop ------------------------------------------------------------
      q(:m01, :multi_hop, "Why did we pick a token bucket over a fixed window counter?", [
        :d_bucket,
        :o_bucket,
        :o_fixedwindow
      ]),
      q(:m02, :multi_hop, "Why don't we rate limit by IP address in nginx?", [
        :o_nginx_ip,
        :ob_nat,
        :d_bucket
      ]),
      q(:m03, :multi_hop, "Why Dagre instead of a force-directed layout?", [
        :d_dagre,
        :o_dagre,
        :o_force
      ]),
      q(:m04, :multi_hop, "What did we build for the token bucket and how did it perform?", [
        :a_limiter,
        :oc_limiter
      ]),
      q(:m05, :multi_hop, "What did we consider instead of SQLite for local storage?", [
        :o_pg_local,
        :d_sqlite
      ]),
      q(:m06, :multi_hop, "What problem made us turn on WAL mode, and did it fix it?", [
        :ob_busy,
        :a_wal,
        :oc_wal
      ]),
      q(
        :m07,
        :multi_hop,
        "Why were session cookies passed over the first time for viewer login?",
        [:d_jwt, :o_cookies]
      ),
      q(:m08, :multi_hop, "Which goal was the merge driver work for?", [
        :a_mergedriver,
        :d_graphjson,
        :g_sync
      ]),
      q(:m09, :multi_hop, "What happened when we shipped the event log import?", [
        :a_eventlog,
        :oc_eventlog
      ]),
      q(:m10, :multi_hop, "Why didn't we sync through a central Postgres server?", [
        :o_central_pg,
        :d_eventlog
      ]),

      # --- temporal / pivot -----------------------------------------------------
      q(:t01, :temporal, "What replaced JWT for viewer auth?", [:d_cookies, :r_auth]),
      q(:t02, :temporal, "What replaced the JSONL event log for sync?", [:d_graphjson, :r_sync]),
      q(:t03, :temporal, "Which decisions have been superseded?", [:d_eventlog, :d_jwt]),
      q(:t04, :temporal, "Why did we pivot on the sync format?", [
        :r_sync,
        :ob_replay,
        :oc_eventlog
      ]),
      q(:t05, :temporal, "How has viewer login changed over time?", [:d_jwt, :r_auth, :d_cookies]),
      q(:t06, :temporal, "What did we try for sync before graph.json?", [:d_eventlog, :o_eventlog]),
      q(:t07, :temporal, "What is the current approach to viewer login?", [:d_cookies]),
      q(:t08, :temporal, "Where did we reverse an earlier decision?", [:r_sync, :r_auth]),

      # --- file / commit --------------------------------------------------------
      q(:f01, :file, "What did we decide about src/db.rs?", [
        :d_sqlite,
        :a_changeid,
        :ob_busy,
        :a_wal
      ]),
      q(:f02, :file, "Which work touched lib/api/rate_limiter.ex?", [:a_limiter]),
      q(:f03, :file, "What changed in web/src/utils/graphProcessing.ts?", [:a_chains]),
      q(:f04, :file, "What do we know about .github/workflows/release.yml?", [:a_dispatch]),
      q(:f05, :file, "Which nodes involve src/records.rs?", [:a_mergedriver]),
      q(:f06, :file, "What was done in lib/web/session_guard.ex and why?", [
        :a_sessionplug,
        :d_cookies
      ]),
      q(:f07, :file, "What does commit 9f3e2a1 do?", [:a_mergedriver]),
      q(:f08, :file, "Which commit turned on WAL mode?", [:a_wal]),

      # --- adversarial ----------------------------------------------------------
      adv(:a01, "What did we decide about GraphQL?", ["graphql"]),
      adv(:a02, "Why did we choose Kubernetes for deployment?", ["kubernetes"]),
      adv(:a03, "What rate limiting did we pick for websocket connections?", ["websocket"]),
      adv(:a04, "What did we decide about push notifications for the mobile app?", [
        "notifications"
      ]),
      adv(:a05, "Why did we drop MongoDB?", ["mongodb"]),
      adv(:a06, "When did we migrate the viewer to Svelte?", ["svelte"]),
      adv(:a07, "What sharding scheme did we choose for Redis?", ["sharding"])
    ]
  end

  @doc """
  The held-out questions (see the moduledoc): same shape as `questions/0`,
  none of the route cue words. Run once after the fixes they were written
  to check; never tuned against.
  """
  def held_out do
    [
      # --- single-hop -----------------------------------------------------------
      q(:h01, :single_hop, "Where does the local CLI keep its data on disk?", [:d_sqlite]),
      q(:h02, :single_hop, "How do we stop a single client from hogging the public API?", [
        :d_bucket
      ]),
      q(:h03, :single_hop, "What layout engine draws the graph in the browser viewer?", [
        :d_dagre
      ]),
      q(:h04, :single_hop, "How many milliseconds does the limiter cost at p99?", [:oc_limiter]),
      q(:h05, :single_hop, "Which login mechanism does the viewer use now?", [:d_cookies]),
      q(
        :h06,
        :single_hop,
        "What goes wrong when the CLI and the server process both open the database?",
        [:ob_busy]
      ),
      q(:h07, :single_hop, "How do teammates see each other's nodes after a git pull?", [
        :d_graphjson
      ]),
      q(
        :h08,
        :single_hop,
        "What stops GitHub from starting the release job when the bot pushes a tag?",
        [:ob_bot_tags]
      ),

      # --- multi-hop ------------------------------------------------------------
      q(
        :h09,
        :multi_hop,
        "Which options for storing the graph on each machine were on the table, and which one won?",
        [:o_sqlite, :o_pg_local, :d_sqlite]
      ),
      q(:h10, :multi_hop, "What did carrier NAT have to do with how we limit API traffic?", [
        :ob_nat,
        :d_bucket,
        :o_nginx_ip
      ]),
      q(:h11, :multi_hop, "What middleware went into the API pipeline and how fast is it?", [
        :a_limiter,
        :oc_limiter
      ]),
      q(
        :h12,
        :multi_hop,
        "After WAL mode went in, how many failures did the concurrent test still show?",
        [:a_wal, :oc_wal]
      ),
      q(:h13, :multi_hop, "What was wrong with the force-directed D3 layout?", [
        :o_force,
        :d_dagre
      ]),
      q(:h14, :multi_hop, "Which goal does the session plug belong to?", [
        :a_sessionplug,
        :d_cookies,
        :g_auth
      ]),

      # --- temporal -------------------------------------------------------------
      q(
        :h15,
        :temporal,
        "What did viewer login look like at first, and what does it look like today?",
        [:d_jwt, :d_cookies]
      ),
      q(:h16, :temporal, "Which sync approach got abandoned, and what took its place?", [
        :d_eventlog,
        :d_graphjson
      ]),
      q(:h17, :temporal, "When did duplicate nodes after a rebase make us rethink sync?", [
        :oc_eventlog,
        :r_sync
      ]),
      q(:h18, :temporal, "Which choices are no longer in force?", [:d_eventlog, :d_jwt]),
      q(
        :h19,
        :temporal,
        "What did we move away from on the auth side, and what pushed us off it?",
        [:d_jwt, :ob_jwt_size, :r_auth]
      ),

      # --- by path or hash -------------------------------------------------------
      q(:h20, :file, "Anything recorded about src/events.rs?", [:a_eventlog]),
      q(:h21, :file, "What went into lib/api/router.ex?", [:a_limiter]),
      q(:h22, :file, "What do we know about migrations/2025-11-02-add-change-id/up.sql?", [
        :a_changeid
      ]),
      q(:h23, :file, "What is .gitattributes for in this repo?", [:a_mergedriver]),
      q(:h24, :file, "What landed in d1e2f3a?", [:a_wal]),

      # --- adversarial ----------------------------------------------------------
      adv(:h25, "Did we ever look at gRPC for the API?", ["grpc"]),
      adv(:h26, "What caching layer sits in front of Elasticsearch?", ["elasticsearch"]),
      adv(:h27, "How do we handle OAuth with Google for the viewer?", ["oauth", "google"]),
      adv(:h28, "What is our backup schedule for the SQLite database?", ["backup", "schedule"])
    ]
  end

  @doc "The route cue words the held-out set must not contain."
  def route_cues do
    routes = DeciduousMcp.Graph.Retrieval.default_routes() ++ [DeciduousMcp.Graph.Related.route()]

    routes
    |> Enum.flat_map(fn
      %{cues: cues} when is_list(cues) -> cues
      _ -> []
    end)
    |> MapSet.new()
  end

  defp q(id, category, question, expect),
    do: %{id: id, category: category, question: question, expect: expect, absent_terms: []}

  defp adv(id, question, absent),
    do: %{id: id, category: :adversarial, question: question, expect: [], absent_terms: absent}

  @doc "Raises unless every key a question or edge names is a fixture node."
  def validate! do
    keys = MapSet.new(nodes(), &elem(&1, 0))
    dup = nodes() |> Enum.map(&elem(&1, 0)) |> then(&(&1 -- Enum.uniq(&1)))
    if dup != [], do: raise(ArgumentError, "duplicate fixture keys: #{inspect(dup)}")

    for {from, to, _, _} <- edges(), k <- [from, to], k not in keys do
      raise ArgumentError, "edge names unknown node #{inspect(k)}"
    end

    text =
      nodes()
      |> Enum.map_join("\n", fn {_, _, title, attrs} ->
        Enum.join([title, attrs[:description], attrs[:commit] | attrs[:files] || []], " ")
      end)
      |> Kernel.<>(Enum.map_join(edges(), "\n", &(elem(&1, 3) || "")))
      |> String.downcase()

    all = questions() ++ held_out()

    for q <- all, t <- q.absent_terms, String.contains?(text, t) do
      raise ArgumentError, "#{q.id}: absent term #{inspect(t)} appears in the fixture"
    end

    ids = Enum.map(all, & &1.id)
    if ids != Enum.uniq(ids), do: raise(ArgumentError, "duplicate question ids")

    cues = route_cues()

    # Retrieval.question_words/1's tokenisation: what a cue is compared with.
    for q <- held_out(),
        w <- q.question |> String.downcase() |> String.split(~r/[^\w-]+/u, trim: true),
        MapSet.member?(cues, w) do
      raise ArgumentError, "held-out #{q.id} uses the route cue word #{inspect(w)}"
    end

    for q <- all do
      for k <- q.expect, k not in keys do
        raise ArgumentError, "question #{q.id} expects unknown node #{inspect(k)}"
      end

      case {q.category, q.expect, q.absent_terms} do
        {:adversarial, [], [_ | _]} ->
          :ok

        {:adversarial, _, _} ->
          raise ArgumentError, "#{q.id}: adversarial needs absent_terms, no expect"

        {_, [_ | _], []} ->
          :ok

        _ ->
          raise ArgumentError, "#{q.id}: needs expect and no absent_terms"
      end
    end

    :ok
  end
end
