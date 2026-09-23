defmodule DeciduousMcp.Web.LocateInputTest do
  @moduledoc """
  SERVER-N5: POST /locate with a NUL in a change_id answered an empty HTTP
  500: the id went into `WHERE change_id IN (...)` and Postgres cannot take
  a NUL in text. Over real HTTP, because the empty 500 is Bandit's answer
  to a crashed plug.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Test.McpHttp

  defp locate(ids) do
    {status, _, body} =
      McpHttp.request("POST", "/locate", Jason.encode!(%{change_ids: ids}), [
        {"content-type", "application/json"}
      ])

    {status, body}
  end

  test "SERVER-N5: a change_id holding a NUL is refused with a sentence, not a 500" do
    assert {422, body} = locate(["ok", "a\u0000b"])
    assert Jason.decode!(body)["error"] =~ "change_ids[1] contains a NUL character"
  end

  test "SERVER-N5: ordinary ids still locate" do
    assert {200, body} = locate(["nothing-holds-this"])
    assert Jason.decode!(body) == %{"workspaces" => []}
  end
end
