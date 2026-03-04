defmodule Deciduex.Commands.CommandLogTest do
  use ExUnit.Case

  alias Deciduex.Commands.CommandLog

  import ExUnit.CaptureIO
  import Deciduex.TestFixtures

  setup do
    create_tables!()
    :ok
  end

  test "shows message when no commands" do
    output = capture_io(fn -> CommandLog.run() end)
    assert output =~ "No commands logged."
  end

  test "renders commands with timestamp and exit code" do
    seed_sample_data!()
    output = capture_io(fn -> CommandLog.run() end)

    assert output =~ "[2024-01-02T10:30:00Z]"
    assert output =~ ~s(deciduous add option "Use JWT")
    assert output =~ "(exit: 0)"
  end

  test "renders commands in reverse chronological order" do
    seed_sample_data!()
    output = capture_io(fn -> CommandLog.run() end)

    lines = String.split(output, "\n", trim: true)
    # Most recent first
    assert Enum.at(lines, 0) =~ "2024-01-02"
    assert Enum.at(lines, 1) =~ "2024-01-01"
  end

  test "respects --limit flag" do
    seed_sample_data!()
    output = capture_io(fn -> CommandLog.run(["--limit", "1"]) end)

    lines = String.split(output, "\n", trim: true)
    assert length(lines) == 1
  end

  test "respects -l flag" do
    seed_sample_data!()
    output = capture_io(fn -> CommandLog.run(["-l", "1"]) end)

    lines = String.split(output, "\n", trim: true)
    assert length(lines) == 1
  end

  test "shows running for nil exit code" do
    insert_command!(%{
      id: 10,
      command: "deciduous serve",
      started_at: "2024-01-01T10:00:00Z",
      exit_code: nil
    })

    output = capture_io(fn -> CommandLog.run() end)
    assert output =~ "(exit: running)"
  end

  test "truncates long commands" do
    long_cmd = String.duplicate("a", 100)

    insert_command!(%{
      id: 10,
      command: long_cmd,
      started_at: "2024-01-01T10:00:00Z"
    })

    output = capture_io(fn -> CommandLog.run() end)
    assert output =~ "..."
    refute output =~ long_cmd
  end
end
