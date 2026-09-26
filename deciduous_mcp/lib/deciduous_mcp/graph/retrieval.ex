defmodule DeciduousMcp.Graph.Retrieval do
  @moduledoc """
  Closed-loop retrieval over the decision graph, the deterministic half of
  Jev-Mem's System-One retrieval (paper section 3.3, appendix B.4-B.5).

  The paper runs `route -> retrieve -> assess -> expand -> reassess` with a
  small model estimating every probability. Here every estimate is a rule
  over the question's words and the graph's own structure, so the same
  question over the same graph always gives the same answer and no model is
  called.

  ## Anchors (paper eq. 16)

  Two rankings of the live nodes in scope, fused by reciprocal rank with
  k = 60:

    * `trigram`: nodes where any question term occurs in the title, the
      description or a metadata value (ILIKE, served by the pg_trgm GIN
      indexes), ranked by pg_trgm `word_similarity` of the question terms to
      the title and to the description.
    * `fts`: `to_tsvector('english', title || ' ' || coalesce(description, ''))
      @@ to_tsquery(term1 | term2 ...)`, ranked by `ts_rank`. This is what
      catches "migrations" for "migration": stemming, which ILIKE has not.

  Two more lists join the fusion when the question asks for them:
  `type_hint`, from words naming a node type or status ("decisions",
  "pending"), re-ranks the text matches of that type; and `recent`, from a
  recency word ("latest"), is the text matches newest first. A question
  whose only words are type/status words ("what goals are still pending")
  has no text search and `type_hint` is its whole anchor list. A question
  with content words that match nothing gets no anchors and lists those
  words in `unmatched_terms`: no type list stands in for a missing topic.

  Words that switch on a route, the depth or recency ("why", "decided",
  "history", "latest") are routing, not content, and are removed from the
  text terms (`route_terms`) unless nothing else is left.

  ## Routes (paper eq. 11-15)

  A route is a relational view with cue words and a weight. It is active
  when a cue appears in the question: `rationale` (why, decided, chose),
  `history` (pivot, replaced, trace), `dependency` (blocked, requires),
  `outcome` (did it work, result); `type_hint` when the question names a
  node type ("which approaches" follows edges to options); `context` is
  always active at a low weight. The expansion budget B is split across active routes: one each,
  the rest in proportion to weight with largest-remainder rounding (eq. 14,
  gamma = 1). The depth cap is 4 when the question asks for a chain
  (trace, history, lineage, ...) and 2 otherwise (eq. 15).

  Edge types alone are not a usable signal on real graphs: on the dev
  database (29,853 nodes) 80,234 of 80,682 edges are `leads_to`, `blocks`
  does not occur and `chosen`/`rejected` together are 218. So an edge route
  scores an edge by its type *and* by the neighbour's type and status.

  ## Expansion and stopping (paper eq. 17-26)

  Each round loads every edge touching the frontier in two queries (out and
  in, neighbour joined, deleted and out-of-scope neighbours dropped in SQL),
  scores each unvisited neighbour through its best route, and admits the
  top candidates (beam 10) whose score is at least 0.25 while their route
  has budget. Score (eq. 23 without the model terms):

      (2.0 * lexical + 1.0 * route_weight * usefulness
       + 1.0 * novelty + 0.5 * min(edge_weight, 1)) / 4.5

  `lexical` is the share of question terms the node contains, `novelty` the
  share it contains that no evidence so far contains. With a recency cue
  ("latest", "recent", ...) eq. 24-25 adjusts it by age in days.

  Retrieval stops, and says which in `stop_reason`, when: every term is
  covered, every active cue route has admitted a node and, on a history
  question, the last round brought in no revisit whose other side is still
  unexplored (`evidence_sufficient`); no candidate scores above the threshold
  (`no_candidate_above_threshold`); the frontier has no unvisited
  neighbours (`frontier_empty`); every route's budget is spent
  (`budget_exhausted`); or the depth cap is reached (`depth_cap`).

  What the model estimated in the paper and nothing estimates here:
  whether the evidence actually answers the question, and contradiction.
  `evidence_sufficient` is term coverage plus route coverage, not a
  judgement that the answer is present.

  ## Extra routes

  A route that does not follow stored edges supplies `expand`:

      %{name: "files", cues: ~w(file files), weight: 1.0,
        expand: fn workspace_id, frontier_ids, visited_ids ->
          [{from_id, %Node{}, usefulness, %{shared_files: [...]}}]
        end}

  An edge route's `edge` is `fn edge_type, neighbour, from_node -> usefulness | nil`.

  Pass it in `opts[:extra_routes]`. Its neighbours are scope-checked,
  scored and budgeted like any other (the scope is applied once, to every
  route's neighbours together, so a route need not know about scopes); the
  extra map is merged into the path step.
  """

  import Ecto.Query

  alias DeciduousMcp.Graph.Nodes
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.{Edge, Node}

  @rrf_k 60
  @lexical_pool 50
  @type_hint_pool 15
  @anchor_limit 15
  @budget 24
  @beam 10
  @threshold 0.25
  @edges_per_round 2000
  @depth_single 2
  @depth_multi 4
  @max_terms 8

  @lambda_lex 2.0
  @lambda_route 1.0
  @lambda_novelty 1.0
  @lambda_edge 0.5
  @lambda_sum @lambda_lex + @lambda_route + @lambda_novelty + @lambda_edge

  @scopes ~w(all active decisions goals observations recent)

  @stop_words ~w(what which how when where why who is are was were do does did
                 the a an in on at to for of and or but not with from by about
                 have has had been being will would could should can may might
                 this that these those it its my our your their we they you i
                 me tell show find get list give search look all any some every
                 please help can over)

  @multi_hop_words ~w(trace history lineage chain evolve evolved evolution path)
  @multi_hop_phrases ["how did we get", "led to", "lead to", "over time", "get here"]
  @recency_words ~w(recent recently latest last newest now current currently today)

  @doc "The scope strings `run/3` accepts."
  def scopes, do: @scopes

  @doc """
  The built-in routes, in allocation order.

  `edge` gets the edge type, the neighbour node and the frontier node the
  edge was reached from, and returns a usefulness in 0..1, or nil when the
  route does not follow that edge.
  """
  def default_routes do
    [
      %{
        name: "rationale",
        cues: ~w(why reason reasons rationale decide decided decision decisions chose choose
             chosen pick picked rejected reject instead tradeoff tradeoffs motivation because
             problem cause caused),
        weight: 1.0,
        edge: &rationale_edge/3
      },
      %{
        name: "history",
        cues: ~w(history pivot pivoted pivots revisit revisited reconsider reconsidered superseded
             supersede changed evolve evolved evolution originally trace lineage replace replaced
             reverse reversed reversal undo undid earlier previous previously),
        weight: 1.0,
        edge: &history_edge/3
      },
      %{
        name: "dependency",
        cues: ~w(blocked blocking blocks blocker blockers depends depend dependency dependencies
             requires require required prerequisite prerequisites waiting stuck enables),
        weight: 1.0,
        edge: &dependency_edge/3
      },
      %{
        name: "outcome",
        cues: ~w(outcome outcomes result results fix fixed work worked perform performed
             performance happened succeed succeeded fail failed effect),
        weight: 1.0,
        edge: &outcome_edge/3
      },
      %{name: "context", cues: :always, weight: 0.25, edge: &context_edge/3}
    ]
  end

  # "why" questions: the choice edges, then the chain that carries a choice
  # (options, decisions, the revisits that reopened them, the observations
  # behind them).
  defp rationale_edge(type, _n, _from) when type in ["chosen", "rejected"], do: 1.0
  defp rationale_edge(_type, %{node_type: "revisit"}, _from), do: 0.9
  defp rationale_edge(_type, %{node_type: t}, _from) when t in ["decision", "option"], do: 0.7
  defp rationale_edge(_type, %{node_type: "observation"}, _from), do: 0.7
  defp rationale_edge(_type, _n, _from), do: nil

  # "how did this change": revisits, what was replaced or dropped, and on
  # either side of a revisit the chain it sits in (the observation that
  # forced it, the decision that replaced the old one).
  defp history_edge(_type, %{node_type: "revisit"}, _from), do: 1.0

  defp history_edge(_type, %{node_type: t}, %{node_type: "revisit"})
       when t in ["decision", "observation", "option"],
       do: 0.9

  defp history_edge(_type, %{status: s}, _from)
       when s in ["superseded", "abandoned", "rejected"],
       do: 0.9

  defp history_edge(type, _n, _from) when type in ["chosen", "rejected"], do: 0.6
  defp history_edge(_type, %{node_type: "decision"}, _from), do: 0.5
  defp history_edge(_type, _n, _from), do: nil

  defp dependency_edge(type, _n, _from) when type in ["requires", "blocks"], do: 1.0
  defp dependency_edge("enables", _n, _from), do: 0.8
  defp dependency_edge(_type, _n, _from), do: nil

  # "did it work": the outcomes hanging off what was found. Only outcomes:
  # the flow is goal -> option -> decision -> action -> outcome, so an
  # outcome is the one node type that records whether something worked.
  defp outcome_edge(_type, %{node_type: "outcome"}, _from), do: 1.0
  defp outcome_edge(_type, _n, _from), do: nil

  defp context_edge(_type, _n, _from), do: 0.3

  # A question that names a node type ("which approaches", "what
  # observations") expands toward nodes of that type. Built per question
  # from the type/status words; not in default_routes because it has no
  # fixed cue list of its own.
  defp type_route(nil), do: []

  defp type_route(hint) do
    [
      %{
        name: "type_hint",
        cues: :hint,
        weight: 1.0,
        edge: fn _type, n, _from -> if hint_matches?(n, hint), do: 1.0 end
      }
    ]
  end

  @doc """
  The search terms of a question: lowercased, split on whitespace, each word
  cut down to letters, digits and `- _ / .` with leading and trailing
  punctuation removed, stop words and words under three characters dropped,
  at most #{@max_terms}.

  `/` and `.` survive inside a word so a path stays a path: the old
  extraction deleted every non-word character, so "src/db.rs" became
  "srcdbrs" and matched nothing (D-eval: all five path questions without a
  second topic word scored 0).
  """
  def extract_terms(question) do
    question
    |> String.downcase()
    |> String.split()
    |> Enum.map(fn word ->
      word
      |> String.replace(~r/[^\w\/.-]/u, "")
      |> String.trim_leading(".")
      |> String.trim_trailing(".")
      |> String.trim("-")
      |> String.trim("/")
    end)
    |> Enum.reject(&(&1 in @stop_words))
    |> Enum.reject(&(String.length(&1) < 3))
    |> Enum.uniq()
    |> Enum.take(@max_terms)
  end

  @doc """
  Run retrieval for `question` in a workspace (or `:global`).

  Options: `:scope` (one of `scopes/0`, default "all"), `:budget` (total
  expansion budget, default #{@budget}), `:anchor_limit` (default
  #{@anchor_limit}), `:routes` (replaces `default_routes/0`),
  `:extra_routes` (appended to them).

  Returns `{:ok, map}` with `:anchors` and `:expanded` (lists of
  `%{node, reached_by, path, score, ranks}`), `:terms`, `:term_hits`,
  `:unmatched_terms`, `:routes`, `:depth_cap`, `:stop_reason`, `:rounds`
  and `:truncated_rounds`; or `{:error, message}`.
  """
  def run(workspace_id, question, opts \\ [])

  def run(_workspace_id, question, _opts) when not is_binary(question) do
    {:error, "question must be a string"}
  end

  def run(workspace_id, question, opts) do
    scope = Keyword.get(opts, :scope, "all")
    budget = Keyword.get(opts, :budget, @budget)
    anchor_limit = Keyword.get(opts, :anchor_limit, @anchor_limit)
    routes = Keyword.get(opts, :routes, default_routes()) ++ Keyword.get(opts, :extra_routes, [])

    with :ok <- check_scope(scope),
         :ok <- check_positive(:budget, budget),
         :ok <- check_positive(:anchor_limit, anchor_limit),
         :ok <- check_routes(routes) do
      do_run(workspace_id, question, scope, budget, anchor_limit, routes)
    end
  end

  @lead_anchors 5

  @doc """
  The hits of a `run/3` result in answer order: the #{@lead_anchors} best
  anchors, then every expanded node (by round, then score), then the
  remaining anchors.

  Anchors and expanded nodes are scored on different scales (RRF against
  eq. 23), so they are not merged by score. Putting all anchors first
  buried what the routes found: on D-eval's fixture "where did we reverse an
  earlier decision" had both revisits at ranks 14 and 16, behind fifteen
  decisions whose only merit was being decisions.
  """
  def ordered(%{anchors: anchors, expanded: expanded}) do
    {lead, rest} = Enum.split(anchors, @lead_anchors)
    lead ++ expanded ++ rest
  end

  defp check_scope(scope) when scope in @scopes, do: :ok

  defp check_scope(scope),
    do: {:error, "unknown scope #{inspect(scope)}; expected one of #{Enum.join(@scopes, ", ")}"}

  defp check_positive(_name, n) when is_integer(n) and n > 0, do: :ok

  defp check_positive(name, n),
    do: {:error, "#{name} must be a positive integer, got #{inspect(n)}"}

  defp check_routes(routes) do
    bad =
      Enum.reject(routes, fn r ->
        is_map(r) and is_binary(r[:name]) and is_number(r[:weight]) and r[:weight] > 0 and
          (r[:cues] == :always or is_list(r[:cues])) and
          (is_function(r[:edge], 3) or is_function(r[:expand], 3))
      end)

    names = Enum.map(routes, & &1[:name])

    cond do
      bad != [] ->
        {:error, "invalid route(s): #{inspect(bad)}"}

      length(Enum.uniq(names)) != length(names) ->
        {:error, "duplicate route names: #{inspect(names)}"}

      true ->
        :ok
    end
  end

  defp do_run(workspace_id, question, scope, budget, anchor_limit, routes) do
    words = question_words(question)
    q = String.downcase(question)

    hint = type_hint(words, q)
    active = active_routes(routes ++ type_route(hint), words)
    budgets = allocate(active, budget)

    {terms, route_terms} = split_terms(extract_terms(question), active, hint)

    depth_cap =
      if Enum.any?(@multi_hop_words, &(&1 in words)) or
           String.contains?(q, @multi_hop_phrases),
         do: @depth_multi,
         else: @depth_single

    recency = if Enum.any?(@recency_words, &(&1 in words)), do: 1.0, else: 0.0

    scope_dyn = scope_dynamic(workspace_id, scope)

    anchors = anchors(terms, hint, recency, scope_dyn, anchor_limit)
    term_hits = term_hits(terms, scope_dyn)
    unmatched = for t <- terms, Map.get(term_hits, t, 0) == 0, do: t

    ctx = %{
      workspace_id: workspace_id,
      terms: terms,
      scope_dyn: scope_dyn,
      active: active,
      recency: recency,
      now: DateTime.utc_now()
    }

    loop =
      expand(ctx, %{
        depth: 0,
        depth_cap: depth_cap,
        frontier: Enum.map(anchors, & &1.node.id),
        visited: MapSet.new(anchors, & &1.node.id),
        nodes: Map.new(anchors, &{&1.node.id, &1.node}),
        last_admitted: Enum.map(anchors, & &1.node),
        paths: Map.new(anchors, &{&1.node.id, []}),
        covered: covered_terms(Enum.map(anchors, & &1.node), terms),
        budgets: budgets,
        spent: Map.new(budgets, fn {k, _} -> {k, 0} end),
        reached_routes: MapSet.new(),
        expanded: [],
        truncated: []
      })

    {:ok,
     %{
       anchors: anchors,
       expanded: Enum.reverse(loop.expanded),
       terms: terms,
       route_terms: route_terms,
       term_hits: term_hits,
       unmatched_terms: unmatched,
       routes:
         Enum.map(active, fn r ->
           %{
             name: r.name,
             weight: r.weight,
             budget: Map.fetch!(budgets, r.name),
             spent: Map.fetch!(loop.spent, r.name)
           }
         end),
       depth_cap: depth_cap,
       stop_reason: loop.stop_reason,
       rounds: loop.depth,
       truncated_rounds: Enum.reverse(loop.truncated)
     }}
  end

  # Words that switched on a route, a multi-hop depth or recency ("why",
  # "decided", "history", "trace", "latest") say how to search, not what
  # for. Searched as text they matched every node that happens to say
  # "choose" or "history": on the dev database "why did we choose postgres"
  # expanded first into "Feature flags with cfg attributes", reached only
  # because it contains "choose". They are dropped from the text terms
  # unless nothing else is left ("what is blocked?" still searches
  # "blocked"). Type and status words ("goals", "pending") are control
  # words too; when they are all there is, the question is structural and
  # is answered by the type/status list alone, with no text search.
  defp split_terms(terms, active, hint) do
    control =
      active
      |> Enum.flat_map(fn
        %{cues: cues} when is_list(cues) -> cues
        _ -> []
      end)
      |> Kernel.++(@multi_hop_words ++ @recency_words ++ hint_words())
      |> MapSet.new()

    {route_terms, content} = Enum.split_with(terms, &MapSet.member?(control, &1))

    cond do
      content != [] -> {content, route_terms}
      hint != nil -> {[], route_terms}
      true -> {terms, []}
    end
  end

  defp question_words(question) do
    question |> String.downcase() |> String.split(~r/[^\w-]+/u, trim: true)
  end

  defp active_routes(routes, words) do
    Enum.filter(routes, fn
      %{cues: :always} -> true
      %{cues: :hint} -> true
      %{cues: cues} -> Enum.any?(cues, &(&1 in words))
    end)
  end

  @doc false
  # Eq. 14 with m = 1 and gamma = 1: one each, the rest by weight, floors
  # first, then the largest remainders (ties in route order).
  def allocate(active, budget) do
    n = length(active)

    if budget < n do
      # Not a silent shrink: fewer slots than routes means some route gets
      # none, and which one would be an arbitrary choice.
      raise ArgumentError,
            "expansion budget #{budget} is smaller than the #{n} active routes"
    end

    rest = budget - n
    total_w = Enum.reduce(active, 0.0, &(&1.weight + &2))

    shares =
      active
      |> Enum.with_index()
      |> Enum.map(fn {r, i} ->
        exact = rest * r.weight / total_w
        {r.name, i, floor(exact), exact - floor(exact)}
      end)

    left = rest - Enum.reduce(shares, 0, fn {_, _, f, _}, acc -> acc + f end)

    bonus =
      shares
      |> Enum.sort_by(fn {_, i, _, frac} -> {-frac, i} end)
      |> Enum.take(left)
      |> MapSet.new(fn {name, _, _, _} -> name end)

    Map.new(shares, fn {name, _, f, _} ->
      {name, 1 + f + if(MapSet.member?(bonus, name), do: 1, else: 0)}
    end)
  end

  # --- Scope ---------------------------------------------------------------

  defp scope_dynamic(workspace_id, scope) do
    base =
      case workspace_id do
        :global -> dynamic([node: n], is_nil(n.deleted_at))
        ws -> dynamic([node: n], n.workspace_id == ^ws and is_nil(n.deleted_at))
      end

    case scope do
      "all" ->
        base

      "active" ->
        dynamic([node: n], ^base and n.status in ["pending", "active"])

      "decisions" ->
        dynamic([node: n], ^base and n.node_type == "decision")

      "goals" ->
        dynamic([node: n], ^base and n.node_type == "goal")

      "observations" ->
        dynamic([node: n], ^base and n.node_type == "observation")

      "recent" ->
        # The 50 newest live nodes, as the tool's description says. The old
        # code put `limit 50` on the search query instead, which meant "the
        # first 50 matches", not "matches among the last 50 nodes".
        recent =
          from(r in Node,
            as: :node,
            where: ^base,
            order_by: [desc: r.inserted_at],
            limit: 50,
            select: r.id
          )

        dynamic([node: n], ^base and n.id in subquery(recent))
    end
  end

  defp nodes_in_scope(scope_dyn) do
    from(n in Node, as: :node, where: ^scope_dyn)
  end

  # --- Anchors -------------------------------------------------------------

  defp anchors(terms, hint, recency, scope_dyn, limit) do
    trigram = trigram_list(terms, scope_dyn)
    fts = fts_list(terms, scope_dyn)

    lexical = Enum.uniq_by(trigram ++ fts, & &1.id)

    # With content terms, a type or status word only re-ranks their matches
    # ("what decisions about auth": the auth matches that are decisions get
    # one more RRF term). Without content terms it is the whole answer
    # ("what goals are still pending"), newest first, as ask_graph did.
    # Only a question with no content terms at all gets the type list on its
    # own. "what did we decide about graphql" on a graph with no graphql
    # returned the 15 newest decisions, which reads as an answer; now it
    # returns nothing and unmatched_terms says ["graphql"].
    hint_list =
      cond do
        hint == nil -> []
        terms == [] -> type_hint_query(hint, scope_dyn)
        true -> Enum.filter(lexical, &hint_matches?(&1, hint))
      end

    # A recency cue ("latest", "recent") is a ranking of its own over the
    # text matches, newest first (the anchor side of paper eq. 24-25).
    recent =
      if recency > 0,
        do: Enum.sort_by(lexical, &{-DateTime.to_unix(&1.inserted_at, :microsecond), &1.id}),
        else: []

    lists = [trigram: trigram, fts: fts, type_hint: hint_list, recent: recent]

    by_id =
      lists
      |> Enum.flat_map(fn {_, nodes} -> nodes end)
      |> Map.new(&{&1.id, &1})

    ranks =
      Enum.reduce(lists, %{}, fn {name, nodes}, acc ->
        nodes
        |> Enum.with_index(1)
        |> Enum.reduce(acc, fn {n, rank}, acc2 ->
          Map.update(acc2, n.id, %{name => rank}, &Map.put(&1, name, rank))
        end)
      end)

    ranks
    |> Enum.map(fn {id, r} ->
      rrf = r |> Map.values() |> Enum.reduce(0.0, fn rank, s -> s + 1 / (@rrf_k + rank) end)
      {id, r, rrf}
    end)
    # Ties: better trigram rank, then better fts rank, then newer, then id.
    |> Enum.sort_by(fn {id, r, rrf} ->
      n = Map.fetch!(by_id, id)

      {-rrf, Map.get(r, :trigram, 1_000_000), Map.get(r, :fts, 1_000_000),
       -DateTime.to_unix(n.inserted_at, :microsecond), id}
    end)
    |> Enum.take(limit)
    |> Enum.map(fn {id, r, rrf} ->
      %{node: Map.fetch!(by_id, id), reached_by: "anchor", path: [], score: rrf, ranks: r}
    end)
  end

  defp any_term_dynamic(terms) do
    # Metadata values, not the JSON text: `metadata::text` includes the key
    # names, and every node add_node writes has "branch", so asking about
    # "branch" matched every node. jsonb_each_text gives top-level values as
    # text (an array value as its JSON, so a file path still matches).
    Enum.reduce(terms, dynamic(false), fn term, acc ->
      pattern = Nodes.contains_pattern(term)

      dynamic(
        [node: n],
        ^acc or ilike(n.title, ^pattern) or ilike(n.description, ^pattern) or
          fragment(
            "EXISTS (SELECT 1 FROM jsonb_each_text(CASE WHEN jsonb_typeof(?) = 'object' THEN ? ELSE '{}'::jsonb END) AS kv WHERE kv.value ILIKE ?)",
            n.metadata,
            n.metadata,
            ^pattern
          )
      )
    end)
  end

  defp trigram_list([], _scope_dyn), do: []

  defp trigram_list(terms, scope_dyn) do
    text = Enum.join(terms, " ")

    nodes_in_scope(scope_dyn)
    |> where(^any_term_dynamic(terms))
    |> order_by([node: n],
      desc:
        fragment(
          "GREATEST(word_similarity(?, ?), word_similarity(?, coalesce(?, '')))",
          ^text,
          n.title,
          ^text,
          n.description
        ),
      desc: n.inserted_at,
      asc: n.id
    )
    |> limit(@lexical_pool)
    |> Repo.all()
  end

  # `'term1' | 'term2'`. Terms are already [\w-]+ (extract_terms), so the
  # quotes cannot be broken out of; quoting keeps `-` from being read as an
  # operator.
  defp tsquery_text(terms), do: Enum.map_join(terms, " | ", &("'" <> &1 <> "'"))

  defp fts_list([], _scope_dyn), do: []

  defp fts_list(terms, scope_dyn) do
    tsq = tsquery_text(terms)

    nodes_in_scope(scope_dyn)
    |> where(
      [node: n],
      fragment(
        "to_tsvector('english'::regconfig, ? || ' ' || coalesce(?, '')) @@ to_tsquery('english'::regconfig, ?)",
        n.title,
        n.description,
        ^tsq
      )
    )
    |> order_by([node: n],
      desc:
        fragment(
          "ts_rank(to_tsvector('english'::regconfig, ? || ' ' || coalesce(?, '')), to_tsquery('english'::regconfig, ?))",
          n.title,
          n.description,
          ^tsq
        ),
      desc: n.inserted_at,
      asc: n.id
    )
    |> limit(@lexical_pool)
    |> Repo.all()
  end

  # Whole words, not substrings: `String.contains?(q, "did")` made every
  # "how did ..." question a question about actions, and "aim" matched
  # "claim".
  @type_words [
    {"decision", ~w(decision decisions decided chose choice choices)},
    {"goal", ~w(goal goals objective objectives target targets aim aims)},
    {"observation", ~w(observation observations noticed learned insight insights)},
    {"action", ~w(action actions implemented built)},
    {"outcome", ~w(outcome outcomes result results succeeded failed)},
    {"option", ~w(option options approach approaches alternative alternatives considered)},
    {"revisit", ~w(pivot pivots pivoted revisit revisits reconsidered)}
  ]

  @status_words [
    {"pending", ~w(pending open todo remaining still)},
    {"completed", ~w(completed done finished)},
    {"rejected", ~w(rejected abandoned dropped)},
    {"active", ~w(active current)}
  ]

  defp hint_words do
    Enum.flat_map(@type_words ++ @status_words, fn {_, cues} -> cues end)
  end

  defp type_hint(words, q) do
    pick = fn table ->
      Enum.find_value(table, fn {value, cues} -> if Enum.any?(cues, &(&1 in words)), do: value end)
    end

    type = pick.(@type_words)
    status = if String.contains?(q, "in progress"), do: "active", else: pick.(@status_words)

    if type || status, do: %{type: type, status: status}, else: nil
  end

  defp hint_matches?(node, %{type: t, status: s}) do
    (t == nil or node.node_type == t) and (s == nil or node.status == s)
  end

  defp type_hint_query(%{type: t, status: s}, scope_dyn) do
    query = nodes_in_scope(scope_dyn)
    query = if t, do: where(query, [node: n], n.node_type == ^t), else: query
    query = if s, do: where(query, [node: n], n.status == ^s), else: query

    query
    |> order_by([node: n], desc: n.inserted_at, asc: n.id)
    |> limit(@type_hint_pool)
    |> Repo.all()
  end

  # --- Term hits -----------------------------------------------------------

  # How many live nodes in scope each term matches, by ILIKE (title,
  # description, metadata values) or by full text (stemmed title +
  # description), counted once per node.
  #
  # Two joins against the unnested terms, UNIONed, then counted: one query.
  # The first version put both tests in one join condition, and the
  # full-text half then ran to_tsvector for every node in the workspace
  # once per term (51 ms for three terms over 1,483 nodes on the dev DB);
  # on its own side of the UNION it is a parameterised scan of idx_nodes_fts.
  defp term_hits([], _scope_dyn), do: %{}

  defp term_hits(terms, scope_dyn) do
    patterns = Enum.map(terms, &Nodes.contains_pattern/1)
    tsqs = Enum.map(terms, &("'" <> &1 <> "'"))

    text =
      from(
        t in fragment(
          "SELECT * FROM unnest(?::text[], ?::text[]) AS u(term, pat)",
          ^terms,
          ^patterns
        ),
        join: n in Node,
        as: :node,
        on:
          ^dynamic(
            [t, node: n],
            ^scope_dyn and
              (ilike(n.title, t.pat) or ilike(n.description, t.pat) or
                 fragment(
                   "EXISTS (SELECT 1 FROM jsonb_each_text(CASE WHEN jsonb_typeof(?) = 'object' THEN ? ELSE '{}'::jsonb END) AS kv WHERE kv.value ILIKE ?)",
                   n.metadata,
                   n.metadata,
                   t.pat
                 ))
          ),
        select: %{term: t.term, id: n.id}
      )

    stemmed =
      from(
        t in fragment("SELECT * FROM unnest(?::text[], ?::text[]) AS u(term, tsq)", ^terms, ^tsqs),
        join: n in Node,
        as: :node,
        on:
          ^dynamic(
            [t, node: n],
            ^scope_dyn and
              fragment(
                "to_tsvector('english'::regconfig, ? || ' ' || coalesce(?, '')) @@ to_tsquery('english'::regconfig, ?)",
                n.title,
                n.description,
                t.tsq
              )
          ),
        select: %{term: t.term, id: n.id}
      )

    counted =
      from(h in subquery(union(text, ^stemmed)),
        group_by: h.term,
        select: {h.term, count(h.id)}
      )
      |> Repo.all()
      |> Map.new()

    Map.new(terms, &{&1, Map.get(counted, &1, 0)})
  end

  # --- Expansion -----------------------------------------------------------

  defp expand(ctx, st) do
    cue_routes = for r <- ctx.active, r.cues != :always, do: r.name

    cond do
      ctx.terms != [] and MapSet.size(st.covered) == length(ctx.terms) and
        Enum.all?(cue_routes, &MapSet.member?(st.reached_routes, &1)) and
          not open_revisit?(ctx, st) ->
        Map.put(st, :stop_reason, "evidence_sufficient")

      st.frontier == [] ->
        Map.put(st, :stop_reason, if(st.depth == 0, do: "no_anchors", else: "frontier_empty"))

      st.depth >= st.depth_cap ->
        Map.put(st, :stop_reason, "depth_cap")

      Enum.all?(st.budgets, fn {name, b} -> st.spent[name] >= b end) ->
        Map.put(st, :stop_reason, "budget_exhausted")

      true ->
        round(ctx, st)
    end
  end

  # The deterministic stand-in for the paper's missing-evidence estimate
  # (eq. 19): on a history question, a revisit that the last round brought
  # in has not been followed to what it led to or what forced it, so the
  # answer is known to be incomplete. "What replaced JWT" found the revisit
  # in round 1 and stopped there, one hop short of the replacement.
  defp open_revisit?(ctx, st) do
    Enum.any?(ctx.active, &(&1.name == "history")) and
      Enum.any?(st.last_admitted, &(&1.node_type == "revisit"))
  end

  defp round(ctx, st) do
    depth = st.depth + 1
    {raw, truncated?} = neighbours(ctx, st.frontier, st.visited)

    st =
      if truncated?,
        do: %{st | truncated: [depth | st.truncated]},
        else: st

    candidates =
      raw
      |> Enum.flat_map(fn cand -> score_candidate(ctx, st, cand, depth) end)
      # Best route per neighbour.
      |> Enum.group_by(& &1.node.id)
      |> Enum.map(fn {_, cs} -> Enum.max_by(cs, &{&1.score, &1.route_rank}) end)
      |> Enum.sort_by(&{-&1.score, &1.node.id})

    {admitted, spent} =
      Enum.reduce(candidates, {[], st.spent}, fn c, {acc, spent} ->
        cond do
          length(acc) >= @beam -> {acc, spent}
          c.score < @threshold -> {acc, spent}
          spent[c.route] >= st.budgets[c.route] -> {acc, spent}
          true -> {[c | acc], Map.update!(spent, c.route, &(&1 + 1))}
        end
      end)

    admitted = Enum.reverse(admitted)

    if admitted == [] do
      st
      |> Map.put(:depth, depth)
      |> Map.put(:stop_reason, "no_candidate_above_threshold")
    else
      paths =
        Enum.reduce(admitted, st.paths, fn c, acc ->
          Map.put(acc, c.node.id, Map.fetch!(acc, c.from) ++ [c.step])
        end)

      expanded =
        Enum.reduce(admitted, st.expanded, fn c, acc ->
          [
            %{
              node: c.node,
              reached_by: c.route,
              path: Map.fetch!(paths, c.node.id),
              score: c.score,
              round: depth
            }
            | acc
          ]
        end)

      expand(ctx, %{
        st
        | depth: depth,
          frontier: Enum.map(admitted, & &1.node.id),
          visited: Enum.reduce(admitted, st.visited, &MapSet.put(&2, &1.node.id)),
          paths: paths,
          covered:
            MapSet.union(st.covered, covered_terms(Enum.map(admitted, & &1.node), ctx.terms)),
          spent: spent,
          reached_routes: Enum.reduce(admitted, st.reached_routes, &MapSet.put(&2, &1.route)),
          nodes: Enum.reduce(admitted, st.nodes, &Map.put(&2, &1.node.id, &1.node)),
          last_admitted: Enum.map(admitted, & &1.node),
          expanded: expanded
      })
    end
  end

  # Every live, in-scope, unvisited neighbour of the frontier: two queries
  # for stored edges (out and in), plus each extra route's expand/3, then
  # one scope check over all of them (in_scope/2).
  # Returns {[{:edge, from, node, edge, direction} | {:custom, route, from,
  # node, usefulness, extra}], truncated?}.
  defp neighbours(ctx, frontier, visited) do
    visited_list = MapSet.to_list(visited)

    edge_routes? = Enum.any?(ctx.active, &Map.has_key?(&1, :edge))

    {out, t1} =
      if edge_routes?, do: edge_neighbours(ctx, frontier, visited_list, :out), else: {[], false}

    {inc, t2} =
      if edge_routes?, do: edge_neighbours(ctx, frontier, visited_list, :in), else: {[], false}

    extra =
      for r <- ctx.active,
          Map.has_key?(r, :expand),
          {from, node, u, extra} <- r.expand.(ctx.workspace_id, frontier, visited_list),
          not MapSet.member?(visited, node.id) do
        {:custom, r, from, node, u, extra}
      end

    {in_scope(ctx, out ++ inc ++ extra), t1 or t2}
  end

  # The one place the scope is enforced on expansion. An extra route's
  # expand/3 gets only the workspace, so it cannot apply scope=decisions or
  # scope=active itself, and asking every route to re-implement the scope
  # is how shared_identifier came to return actions under scope=decisions
  # and completed nodes under scope=active. edge_neighbours/4 also filters
  # in SQL, but only so out-of-scope edges do not use up the per-round cap;
  # correctness does not depend on it.
  defp in_scope(_ctx, []), do: []

  defp in_scope(ctx, raw) do
    ids = raw |> Enum.map(&neighbour_id/1) |> Enum.uniq()

    keep =
      nodes_in_scope(ctx.scope_dyn)
      |> where([node: n], n.id in type(^ids, {:array, :binary_id}))
      |> select([node: n], n.id)
      |> Repo.all()
      |> MapSet.new()

    Enum.filter(raw, &MapSet.member?(keep, neighbour_id(&1)))
  end

  defp neighbour_id({:edge, _from, node, _e, _dir}), do: node.id
  defp neighbour_id({:custom, _r, _from, node, _u, _extra}), do: node.id

  defp edge_neighbours(ctx, frontier, visited_list, direction) do
    base =
      case direction do
        :out ->
          from(e in Edge,
            join: n in Node,
            as: :node,
            on: n.id == e.to_node_id,
            where: e.from_node_id in type(^frontier, {:array, :binary_id}),
            select: {e, n, e.from_node_id}
          )

        :in ->
          from(e in Edge,
            join: n in Node,
            as: :node,
            on: n.id == e.from_node_id,
            where: e.to_node_id in type(^frontier, {:array, :binary_id}),
            select: {e, n, e.to_node_id}
          )
      end

    rows =
      base
      |> where(^ctx.scope_dyn)
      |> where([node: n], n.id not in type(^visited_list, {:array, :binary_id}))
      # Newest edges first, so a hub past the cap keeps its recent context.
      |> order_by([e], desc: e.inserted_at, asc: e.id)
      |> limit(^(@edges_per_round + 1))
      |> Repo.all()

    truncated? = length(rows) > @edges_per_round

    {rows
     |> Enum.take(@edges_per_round)
     |> Enum.map(fn {e, n, from} -> {:edge, from, n, e, direction} end), truncated?}
  end

  defp score_candidate(ctx, st, {:edge, from, node, edge, direction}, depth) do
    ctx.active
    |> Enum.with_index()
    |> Enum.flat_map(fn {r, i} ->
      with true <- Map.has_key?(r, :edge),
           u when is_number(u) <- r.edge.(edge.edge_type, node, Map.fetch!(st.nodes, from)) do
        [
          candidate(ctx, st, node, from, r, i, u, edge.weight, depth, %{
            from: from,
            edge_type: edge.edge_type,
            direction: Atom.to_string(direction),
            route: r.name
          })
        ]
      else
        _ -> []
      end
    end)
  end

  defp score_candidate(ctx, st, {:custom, r, from, node, u, extra}, depth) do
    i = Enum.find_index(ctx.active, &(&1.name == r.name))

    [
      candidate(
        ctx,
        st,
        node,
        from,
        r,
        i,
        u,
        1.0,
        depth,
        Map.merge(%{from: from, edge_type: nil, direction: nil, route: r.name}, extra)
      )
    ]
  end

  defp candidate(ctx, st, node, from, route, route_rank, usefulness, edge_weight, _depth, step) do
    matched = covered_terms([node], ctx.terms)
    n_terms = max(length(ctx.terms), 1)
    lexical = MapSet.size(matched) / n_terms
    novelty = MapSet.size(MapSet.difference(matched, st.covered)) / n_terms
    w = min(edge_weight || 1.0, 1.0)

    s =
      (@lambda_lex * lexical + @lambda_route * route.weight * usefulness +
         @lambda_novelty * novelty + @lambda_edge * w) / @lambda_sum

    s =
      if ctx.recency > 0 do
        age_days = max(0, DateTime.diff(ctx.now, node.inserted_at, :second)) / 86_400
        rho = 1 / (1 + age_days)
        (s + 0.1 * ctx.recency * rho) / (1 + 0.1 * ctx.recency)
      else
        s
      end

    %{
      node: node,
      from: from,
      route: route.name,
      route_rank: -route_rank,
      score: Float.round(s, 4),
      step: step
    }
  end

  @doc false
  def covered_terms(nodes, terms) do
    Enum.reduce(nodes, MapSet.new(), fn node, acc ->
      text = node_text(node)

      Enum.reduce(terms, acc, fn t, a ->
        if String.contains?(text, t), do: MapSet.put(a, t), else: a
      end)
    end)
  end

  defp node_text(node) do
    values =
      case node.metadata do
        m when is_map(m) ->
          Enum.map(m, fn
            {_k, v} when is_binary(v) -> v
            {_k, v} -> Jason.encode!(v)
          end)

        _ ->
          []
      end

    [node.title, node.description || "" | values]
    |> Enum.join(" ")
    |> String.downcase()
  end
end
