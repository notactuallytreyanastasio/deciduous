defmodule DeciduousMcp.Eval.AskGraphEvalTest do
  @moduledoc """
  The ask_graph retrieval eval as a test. Tagged `:eval` and excluded by
  default (test_helper.exs): it measures, it does not gate. Run it with

      mix test --only eval

  The fixture check below is not tagged: a question naming a node that does
  not exist, or an "absent" term the fixture does contain, would make every
  score after it meaningless, so that runs with the suite.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Eval.{AskGraph, AskGraphFixture}

  test "the fixture is consistent" do
    assert AskGraphFixture.validate!() == :ok
    qs = AskGraphFixture.questions()
    assert length(qs) in 30..50

    for cat <- [:single_hop, :multi_hop, :temporal, :file, :adversarial] do
      assert Enum.any?(qs, &(&1.category == cat)), "no #{cat} questions"
    end
  end

  test "the held-out set has at least 20 questions and no route cue word" do
    held = AskGraphFixture.held_out()
    assert length(held) >= 20

    cues = AskGraphFixture.route_cues()

    for q <- held,
        w <- q.question |> String.downcase() |> String.split(~r/[^\w-]+/u, trim: true) do
      refute MapSet.member?(cues, w), "#{q.id} uses cue word #{w}"
    end
  end

  test "the probe set: 12+ adversarial, 12+ answerable with a rare in-graph word" do
    probe = AskGraphFixture.probe_set()
    {adv, ans} = Enum.split_with(probe, &(&1.category == :adversarial))
    assert length(adv) >= 12
    assert length(ans) >= 12
    assert Enum.all?(ans, &is_binary(&1[:rare_term]))
  end

  test "adversarial: the absent term named AND at most 3 results, or no results" do
    # The old criterion passed a03 and a04 with 21 and 22 unrelated
    # results because unmatched_terms named the absent word.
    assert AskGraph.adversarial_pass?(0, [], ["graphql"])
    assert AskGraph.adversarial_pass?(3, ["graphql"], ["graphql"])
    refute AskGraph.adversarial_pass?(4, ["graphql"], ["graphql"])
    refute AskGraph.adversarial_pass?(21, ["websocket"], ["websocket"])
    refute AskGraph.adversarial_pass?(1, [], ["graphql"])
    refute AskGraph.adversarial_pass?(2, ["oauth"], ["oauth", "google"])

    # The old one, kept for the report.
    assert AskGraph.adversarial_pass_old?(21, ["websocket"], ["websocket"])
  end

  @tag :eval
  @tag timeout: 300_000
  test "ask_graph retrieval eval" do
    result = AskGraph.run()
    IO.puts("\n" <> AskGraph.format(result))

    # Every question was asked and scored; the numbers themselves are the
    # output, not a pass/fail line.
    assert length(result.questions) == length(AskGraphFixture.questions())
    assert is_float(result.summary.recall10)
  end
end
