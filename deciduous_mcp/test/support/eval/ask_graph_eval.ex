defmodule DeciduousMcp.Eval.AskGraph do
  @moduledoc """
  Retrieval eval for the `ask_graph` tool.

  Loads `DeciduousMcp.Eval.AskGraphFixture` into a fresh workspace through
  the MCP tools (`add_node`, `add_edge`) and asks every question through
  `tools/call ask_graph`, the same router, session and component path an
  MCP client uses. Scoring reads only the tool's JSON answer, so it keeps
  working whatever ask_graph does inside.

  Metrics, over the ranked `results[].id` list:

    * recall@5 / recall@10 -- share of a question's expected nodes in the
      first 5 / 10 results, averaged over questions
    * MRR -- 1 / rank of the first expected node (0 when none is returned)
    * context recall -- share of expected nodes anywhere in the answer,
      including the `connects_to` / `connected_from` neighbours listed
      under each result: what a reader of the whole answer could find
    * adversarial pass -- the question is about something the graph never
      decided. It passes when no results come back, or when the answer's
      top-level `unmatched_terms` names every absent term AND at most
      three results come back. The old criterion (the terms named, any
      number of results) passed "websocket" with 21 unrelated results; it
      is still computed and reported as "adv old" so the two can be
      compared.

  The held-out questions (`AskGraphFixture.held_out/0`, written without
  any route cue word) are asked in the same run and reported in a table of
  their own.

  The caller owns the database: run it inside a shared sandbox that is
  rolled back afterwards (the ExUnit test and `mix deciduous.eval` do).
  """

  alias DeciduousMcp.Eval.AskGraphFixture, as: Fixture
  alias DeciduousMcp.Test.McpClient

  @workspace "eval-ask-graph"

  @doc """
  Loads the fixture and runs every question. Returns
  `%{summary: map, by_category: map, questions: [map]}`.

  Options: `:workspace` (default #{inspect(@workspace)}), `:only` (list of
  categories).
  """
  def run(opts \\ []) do
    :ok = Fixture.validate!()
    ws = Keyword.get(opts, :workspace, @workspace)
    client = McpClient.connect(name: "ask-graph-eval")

    {ids, load_ms} = timed(fn -> load!(client, ws) end)

    questions =
      case opts[:only] do
        nil -> Fixture.questions()
        cats -> Enum.filter(Fixture.questions(), &(&1.category in cats))
      end

    held =
      case opts[:only] do
        nil -> Fixture.held_out()
        cats -> Enum.filter(Fixture.held_out(), &(&1.category in cats))
      end

    rows = Enum.map(questions, &ask_and_score(client, ws, ids, &1))
    held_rows = Enum.map(held, &ask_and_score(client, ws, ids, &1))

    %{
      summary: aggregate(rows) |> Map.put(:load_ms, load_ms) |> Map.put(:nodes, map_size(ids)),
      by_category: by_category(rows),
      questions: rows,
      held_out: %{
        summary: aggregate(held_rows),
        by_category: by_category(held_rows),
        questions: held_rows
      }
    }
  end

  defp by_category(rows),
    do: rows |> Enum.group_by(& &1.category) |> Map.new(fn {c, rs} -> {c, aggregate(rs)} end)

  @adversarial_max_results 3

  @doc """
  The adversarial criterion: no results, or every absent term named in
  `unmatched_terms` and at most #{@adversarial_max_results} results.
  Naming the term while returning a page of loosely related nodes is not a
  pass: a reader sees the page, not the list.
  """
  def adversarial_pass?(result_count, unmatched, absent) do
    result_count == 0 or
      (named_all?(unmatched, absent) and result_count <= @adversarial_max_results)
  end

  @doc "The criterion before it was tightened: no results, or every absent term named."
  def adversarial_pass_old?(result_count, unmatched, absent),
    do: result_count == 0 or named_all?(unmatched, absent)

  defp named_all?(unmatched, absent) do
    got = Enum.map(unmatched, &String.downcase/1)
    Enum.all?(absent, &(&1 in got))
  end

  # --- loading --------------------------------------------------------------

  defp load!(client, ws) do
    ids =
      Map.new(Fixture.nodes(), fn {key, type, title, attrs} ->
        args =
          %{"workspace" => ws, "node_type" => type, "title" => title, "branch" => "main"}
          |> put_opt("description", attrs[:description])
          |> put_opt("status", attrs[:status])
          |> put_opt("files", attrs[:files])
          |> put_opt("commit", attrs[:commit])

        %{"id" => id} = McpClient.call!(client, "add_node", args)
        {key, id}
      end)

    for {from, to, type, rationale} <- Fixture.edges() do
      args =
        %{
          "workspace" => ws,
          "from_node_id" => Map.fetch!(ids, from),
          "to_node_id" => Map.fetch!(ids, to),
          "edge_type" => type
        }
        |> put_opt("rationale", rationale)

      McpClient.call!(client, "add_edge", args)
    end

    ids
  end

  defp put_opt(map, _k, nil), do: map
  defp put_opt(map, k, v), do: Map.put(map, k, v)

  # --- asking and scoring ---------------------------------------------------

  defp ask_and_score(client, ws, ids, q) do
    by_id = Map.new(ids, fn {k, id} -> {id, k} end)

    {answer, ms} =
      timed(fn ->
        McpClient.call!(client, "ask_graph", %{"workspace" => ws, "question" => q.question})
      end)

    results =
      case answer do
        %{"results" => rs} when is_list(rs) -> rs
        other -> raise "ask_graph answered without a results list: #{inspect(other, limit: 20)}"
      end

    ranked = Enum.map(results, &Map.fetch!(&1, "id"))
    ranked_keys = Enum.map(ranked, &Map.get(by_id, &1, :unknown))
    expect_ids = Enum.map(q.expect, &Map.fetch!(ids, &1))

    context_ids =
      results
      |> Enum.flat_map(fn r ->
        [r["id"]] ++
          Enum.map(r["connects_to"] || [], & &1["node_id"]) ++
          Enum.map(r["connected_from"] || [], & &1["node_id"])
      end)
      |> MapSet.new()

    base = %{
      id: q.id,
      category: q.category,
      question: q.question,
      ms: ms,
      result_count: length(results),
      # Diagnostic only, when the tool reports them; never scored.
      search_terms: answer["search_terms"],
      top5: Enum.take(ranked_keys, 5)
    }

    if q.category == :adversarial do
      unmatched =
        case answer["unmatched_terms"] do
          l when is_list(l) -> l
          _ -> []
        end

      n = length(results)

      Map.merge(base, %{
        expect: [],
        absent_terms: q.absent_terms,
        adversarial_pass: adversarial_pass?(n, unmatched, q.absent_terms),
        adversarial_pass_old: adversarial_pass_old?(n, unmatched, q.absent_terms),
        reported_unmatched: named_all?(unmatched, q.absent_terms)
      })
    else
      rank = Enum.find_index(ranked, &(&1 in expect_ids))

      Map.merge(base, %{
        expect: q.expect,
        recall5: recall(expect_ids, Enum.take(ranked, 5)),
        recall10: recall(expect_ids, Enum.take(ranked, 10)),
        rr: if(rank, do: 1 / (rank + 1), else: 0.0),
        context_recall: recall(expect_ids, context_ids),
        # Expected nodes absent from every rank, not just the top 10.
        missed: for(k <- q.expect, Map.fetch!(ids, k) not in ranked, do: k)
      })
    end
  end

  defp recall(expect, got), do: Enum.count(expect, &(&1 in got)) / length(expect)

  defp aggregate(rows) do
    {adv, ret} = Enum.split_with(rows, &(&1.category == :adversarial))
    lat = rows |> Enum.map(& &1.ms) |> Enum.sort()

    %{
      questions: length(rows),
      recall5: mean(ret, :recall5),
      recall10: mean(ret, :recall10),
      mrr: mean(ret, :rr),
      context_recall: mean(ret, :context_recall),
      adversarial: length(adv),
      adversarial_pass: Enum.count(adv, & &1.adversarial_pass),
      adversarial_pass_old: Enum.count(adv, & &1.adversarial_pass_old),
      mean_results: mean(rows, :result_count),
      latency_ms_p50: percentile(lat, 0.5),
      latency_ms_max: List.last(lat)
    }
  end

  defp mean([], _), do: nil
  defp mean(rows, k), do: Enum.sum(Enum.map(rows, &Map.fetch!(&1, k))) / length(rows)

  defp percentile([], _), do: nil

  defp percentile(sorted, p),
    do: Enum.at(sorted, min(length(sorted) - 1, trunc(p * length(sorted))))

  defp timed(fun) do
    t0 = System.monotonic_time(:microsecond)
    v = fun.()
    {v, Float.round((System.monotonic_time(:microsecond) - t0) / 1000, 1)}
  end

  # --- report ---------------------------------------------------------------

  @doc "Formats a `run/1` result as plain text."
  def format(%{summary: s, by_category: by_cat, questions: rows} = result) do
    header =
      "ask_graph retrieval eval: #{s.questions} questions over #{s.nodes} nodes " <>
        "(fixture loaded through MCP in #{s.load_ms}ms)\n\n"

    main = table(s, by_cat) <> "\n\nper question:\n" <> detail(rows) <> "\n"

    case result[:held_out] do
      %{questions: [_ | _] = hrows, summary: hs, by_category: hcat} ->
        header <>
          main <>
          "\nheld-out set: #{hs.questions} questions written without any route cue word " <>
          "(reported separately; not used to tune routes)\n\n" <>
          table(hs, hcat) <> "\n\nper question:\n" <> detail(hrows) <> "\n"

      _ ->
        header <> main
    end
  end

  defp table(s, by_cat) do
    [
      pad([
        "category",
        "n",
        "R@5",
        "R@10",
        "MRR",
        "ctxR",
        "adv pass",
        "adv old",
        "avg results",
        "p50 ms"
      ])
      | for cat <- [:single_hop, :multi_hop, :temporal, :file, :adversarial, :all],
            a = if(cat == :all, do: s, else: by_cat[cat]),
            a != nil do
          pad([
            to_string(cat),
            a.questions,
            f(a.recall5),
            f(a.recall10),
            f(a.mrr),
            f(a.context_recall),
            if(a.adversarial > 0, do: "#{a.adversarial_pass}/#{a.adversarial}", else: "-"),
            if(a.adversarial > 0, do: "#{a.adversarial_pass_old}/#{a.adversarial}", else: "-"),
            f(a.mean_results),
            a.latency_ms_p50
          ])
        end
    ]
    |> Enum.join("\n")
  end

  defp detail(rows) do
    Enum.map_join(rows, "\n", fn r ->
      score =
        if r.category == :adversarial,
          do:
            "#{if r.adversarial_pass, do: "PASS", else: "FAIL"} " <>
              "(#{r.result_count} results; old #{if r.adversarial_pass_old, do: "PASS", else: "FAIL"})",
          else: "R@10 #{f(r.recall10)} RR #{f(r.rr)}" <> missed(r.missed)

      "  #{r.id} #{String.pad_trailing(score, 44)} #{r.question}"
    end)
  end

  defp missed([]), do: ""
  defp missed(ks), do: " not returned: " <> Enum.map_join(ks, ",", &to_string/1)

  defp f(nil), do: "-"
  defp f(x) when is_float(x), do: :erlang.float_to_binary(x, decimals: 3)
  defp f(x), do: to_string(x)

  defp pad(cols) do
    [first | rest] = Enum.map(cols, &to_string/1)
    String.pad_trailing(first, 12) <> Enum.map_join(rest, "", &String.pad_leading(&1, 12))
  end
end
