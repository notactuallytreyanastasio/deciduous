defmodule Deciduex.Commands.ServeTest do
  use ExUnit.Case

  import Plug.Conn
  import Plug.Test

  import Deciduex.TestFixtures

  alias Deciduex.Serve.Router

  setup do
    create_tables!()
    seed_sample_data!()
    :ok
  end

  describe "API routes" do
    test "GET /api/graph returns graph data" do
      conn = conn(:get, "/api/graph") |> Router.call([])

      assert conn.status == 200
      assert get_resp_header(conn, "content-type") == ["application/json; charset=utf-8"]

      body = Jason.decode!(conn.resp_body)
      assert body["ok"] == true
      assert is_map(body["data"])
      assert is_list(body["data"]["nodes"])
      assert is_list(body["data"]["edges"])
    end

    test "GET /api/commands returns command log" do
      conn = conn(:get, "/api/commands") |> Router.call([])

      assert conn.status == 200

      body = Jason.decode!(conn.resp_body)
      assert body["ok"] == true
      assert is_list(body["data"])
    end

    test "GET /api/documents returns document list" do
      conn = conn(:get, "/api/documents") |> Router.call([])

      assert conn.status == 200

      body = Jason.decode!(conn.resp_body)
      assert body["ok"] == true
      assert is_list(body["data"])
    end

    test "GET /api/documents with node_id filter" do
      conn = conn(:get, "/api/documents?node_id=1") |> Router.call([])

      assert conn.status == 200

      body = Jason.decode!(conn.resp_body)
      assert body["ok"] == true
    end
  end

  describe "SPA route" do
    test "GET / returns HTML viewer" do
      conn = conn(:get, "/") |> Router.call([])

      assert conn.status == 200
      assert get_resp_header(conn, "content-type") == ["text/html; charset=utf-8"]
      assert conn.resp_body =~ "Deciduous"
    end

    test "GET /some/path returns HTML (SPA fallback)" do
      conn = conn(:get, "/some/path") |> Router.call([])

      assert conn.status == 200
      assert conn.resp_body =~ "Deciduous"
    end
  end

  describe "404 handling" do
    test "POST to unknown route returns 404" do
      conn = conn(:post, "/unknown") |> Router.call([])

      assert conn.status == 404
    end
  end
end
