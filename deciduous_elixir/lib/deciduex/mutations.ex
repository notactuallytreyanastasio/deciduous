defmodule Deciduex.Mutations do
  @moduledoc """
  Write operations for the decision graph database.
  """

  alias Ecto.Adapters.SQL
  alias Deciduex.Repo

  @doc """
  Create a new decision node with all metadata fields.

  Returns `{:ok, id}` on success, `{:error, reason}` on failure.
  """
  def create_node(attrs) do
    change_id = UUID.uuid4()
    now = DateTime.utc_now() |> DateTime.to_iso8601()

    metadata_json = build_metadata_json(attrs)

    result =
      SQL.query(
        Repo,
        """
        INSERT INTO decision_nodes
          (change_id, node_type, title, description, status, created_at, updated_at, metadata_json)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        """,
        [
          change_id,
          attrs[:node_type],
          attrs[:title],
          attrs[:description],
          "pending",
          attrs[:created_at] || now,
          now,
          metadata_json
        ]
      )

    case result do
      {:ok, _} ->
        # Get the last inserted row ID
        {:ok, %{rows: [[id]]}} = SQL.query(Repo, "SELECT last_insert_rowid()", [])
        {:ok, id}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Create an edge between two nodes.
  """
  def create_edge(from_id, to_id, edge_type, rationale) do
    now = DateTime.utc_now() |> DateTime.to_iso8601()

    # Get change_ids for both nodes
    from_change_id = get_node_change_id(from_id)
    to_change_id = get_node_change_id(to_id)

    result =
      SQL.query(
        Repo,
        """
        INSERT INTO decision_edges
          (from_node_id, to_node_id, from_change_id, to_change_id, edge_type, weight, rationale, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        """,
        [from_id, to_id, from_change_id, to_change_id, edge_type, 1.0, rationale, now]
      )

    case result do
      {:ok, _} ->
        {:ok, %{rows: [[id]]}} = SQL.query(Repo, "SELECT last_insert_rowid()", [])
        {:ok, id}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Delete an edge between two nodes.
  """
  def delete_edge(from_id, to_id) do
    result =
      SQL.query(
        Repo,
        "DELETE FROM decision_edges WHERE from_node_id = ? AND to_node_id = ?",
        [from_id, to_id]
      )

    case result do
      {:ok, %{num_rows: n}} when n > 0 -> {:ok, n}
      {:ok, %{num_rows: 0}} -> {:error, :not_found}
      {:error, reason} -> {:error, reason}
    end
  end

  @doc """
  Update a node's status.
  """
  def update_status(id, status) when status in ~w(pending active superseded abandoned) do
    now = DateTime.utc_now() |> DateTime.to_iso8601()

    result =
      SQL.query(
        Repo,
        "UPDATE decision_nodes SET status = ?, updated_at = ? WHERE id = ?",
        [status, now, id]
      )

    case result do
      {:ok, %{num_rows: 1}} -> :ok
      {:ok, %{num_rows: 0}} -> {:error, :not_found}
      {:error, reason} -> {:error, reason}
    end
  end

  def update_status(_id, status), do: {:error, {:invalid_status, status}}

  @doc """
  Update a node's prompt in metadata_json.
  """
  def update_prompt(id, prompt) do
    now = DateTime.utc_now() |> DateTime.to_iso8601()

    # Get current metadata
    case get_node_metadata(id) do
      nil ->
        {:error, :not_found}

      metadata ->
        new_metadata =
          metadata
          |> Map.put("prompt", prompt)
          |> Jason.encode!()

        result =
          SQL.query(
            Repo,
            "UPDATE decision_nodes SET metadata_json = ?, updated_at = ? WHERE id = ?",
            [new_metadata, now, id]
          )

        case result do
          {:ok, %{num_rows: 1}} -> :ok
          {:ok, %{num_rows: 0}} -> {:error, :not_found}
          {:error, reason} -> {:error, reason}
        end
    end
  end

  @doc """
  Delete a node and all its edges.
  """
  def delete_node(id) do
    # Delete edges first
    SQL.query(
      Repo,
      "DELETE FROM decision_edges WHERE from_node_id = ? OR to_node_id = ?",
      [id, id]
    )

    # Delete the node
    result = SQL.query(Repo, "DELETE FROM decision_nodes WHERE id = ?", [id])

    case result do
      {:ok, %{num_rows: 1}} -> :ok
      {:ok, %{num_rows: 0}} -> {:error, :not_found}
      {:error, reason} -> {:error, reason}
    end
  end

  @doc """
  Log a command to the command_log table.
  """
  def log_command(command, args, exit_code) do
    now = DateTime.utc_now() |> DateTime.to_iso8601()
    full_command = Enum.join([command | args], " ")

    SQL.query(
      Repo,
      "INSERT INTO command_log (command, exit_code, started_at) VALUES (?, ?, ?)",
      [full_command, exit_code, now]
    )

    :ok
  end

  # Private helpers

  defp build_metadata_json(attrs) do
    meta =
      %{}
      |> maybe_put(:confidence, attrs[:confidence])
      |> maybe_put(:commit, attrs[:commit])
      |> maybe_put(:prompt, attrs[:prompt])
      |> maybe_put(:branch, attrs[:branch])
      |> maybe_put_files(attrs[:files])

    if map_size(meta) == 0, do: nil, else: Jason.encode!(meta)
  end

  defp maybe_put(map, _key, nil), do: map
  defp maybe_put(map, key, value), do: Map.put(map, key, value)

  defp maybe_put_files(map, nil), do: map

  defp maybe_put_files(map, files) do
    file_list = files |> String.split(",") |> Enum.map(&String.trim/1)
    Map.put(map, :files, file_list)
  end

  defp get_node_change_id(id) do
    case SQL.query(Repo, "SELECT change_id FROM decision_nodes WHERE id = ?", [id]) do
      {:ok, %{rows: [[change_id]]}} -> change_id
      _ -> nil
    end
  end

  defp get_node_metadata(id) do
    case SQL.query(Repo, "SELECT metadata_json FROM decision_nodes WHERE id = ?", [id]) do
      {:ok, %{rows: [[nil]]}} -> %{}
      {:ok, %{rows: [[json]]}} -> Jason.decode!(json)
      {:ok, %{rows: []}} -> nil
      _ -> nil
    end
  end
end
