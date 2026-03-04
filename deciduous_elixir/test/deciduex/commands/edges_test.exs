defmodule Deciduex.Commands.EdgesTest do
  use ExUnit.Case

  alias Deciduex.Commands.Edges

  import ExUnit.CaptureIO
  import Deciduex.TestFixtures

  setup do
    create_tables!()
    :ok
  end

  test "shows message when no edges" do
    output = capture_io(fn -> Edges.run() end)

    assert output =~ "No edges found"
    assert output =~ "deciduous link"
  end

  test "renders edges with correct columns" do
    seed_sample_data!()
    output = capture_io(fn -> Edges.run() end)

    assert output =~ "ID"
    assert output =~ "FROM"
    assert output =~ "TO"
    assert output =~ "TYPE"
    assert output =~ "RATIONALE"
    assert output =~ String.duplicate("-", 70)
  end

  test "renders edge data" do
    seed_sample_data!()
    output = capture_io(fn -> Edges.run() end)

    assert output =~ "leads_to"
    assert output =~ "possible approach"
    assert output =~ "chosen"
    assert output =~ "JWT is standard"
  end
end
