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
