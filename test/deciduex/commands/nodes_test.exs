defmodule Deciduex.Commands.NodesTest do
  use ExUnit.Case

  alias Deciduex.Commands.Nodes

  import ExUnit.CaptureIO
  import Deciduex.TestFixtures

  setup do
    create_tables!()
    seed_sample_data!()
    :ok
  end

  test "renders all nodes with count header" do
    output = capture_io(fn -> Nodes.run([]) end)

    assert output =~ "3 nodes:"
    assert output =~ "Add auth"
    assert output =~ "Use JWT"
    assert output =~ "Choose JWT"
  end

  test "renders column headers" do
    output = capture_io(fn -> Nodes.run([]) end)

    assert output =~ "ID"
    assert output =~ "TYPE"
    assert output =~ "STATUS"
    assert output =~ "TITLE"
  end

  test "filters by type" do
    output = capture_io(fn -> Nodes.run(["-t", "goal"]) end)

    assert output =~ "1 nodes:"
    assert output =~ "Add auth"
    refute output =~ "Use JWT"
  end

  test "filters by branch" do
    output = capture_io(fn -> Nodes.run(["-b", "feature-auth"]) end)

    assert output =~ "2 nodes:"
    assert output =~ "Use JWT"
    assert output =~ "Choose JWT"
    refute output =~ "Add auth"
  end

  test "combined type and branch filter" do
    output =
      capture_io(fn -> Nodes.run(["-t", "option", "-b", "feature-auth"]) end)

    assert output =~ "1 nodes:"
    assert output =~ "Use JWT"
    refute output =~ "Choose JWT"
  end
end
