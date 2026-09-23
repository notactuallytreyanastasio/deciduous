defmodule DeciduousMcp.Graph.Documents do
  @moduledoc """
  Reading document attachments and their content.
  """
  import Ecto.Query

  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.Document
  alias DeciduousMcp.Storage

  @doc """
  Fetches a document's metadata and bytes.

  `id` is either the document's UUID or a content hash. The hash form exists
  because the same file is attached in more than one project — asking by hash
  returns the content without having to know which attachment you meant.

  `workspace_id:` limits both forms to one workspace, for a client pinned by
  header. Without it a pinned repo read a neighbour's attachment by id, or
  by the hash of any file it could guess. A document whose node is deleted
  is not found either way: the delete hides the node's content, and an
  attachment is part of it.
  """
  def fetch(id, opts \\ []) do
    with {:ok, doc} <- lookup(id, opts[:workspace_id]) do
      cond do
        doc.content_missing -> {:error, :content_missing}
        true -> load_content(doc)
      end
    end
  end

  @doc """
  Lists a node's attachments. Content is not loaded; `content_missing` says
  whether asking for it would succeed.
  """
  def for_node(node_id) do
    Document
    |> where([d], d.node_id == ^node_id and is_nil(d.detached_at))
    |> order_by([d], asc: d.inserted_at)
    |> Repo.all()
  end

  defp lookup(id, workspace_id) do
    base =
      from(d in Document,
        join: n in assoc(d, :node),
        where: is_nil(n.deleted_at)
      )

    base = if workspace_id, do: where(base, [d], d.workspace_id == ^workspace_id), else: base

    result =
      if uuid?(id) do
        Repo.one(where(base, [d], d.id == ^id))
      else
        base
        |> where([d], d.content_hash == ^String.downcase(id))
        |> order_by([d], asc: d.content_missing, asc: d.inserted_at)
        |> limit(1)
        |> Repo.one()
      end

    case result do
      nil -> {:error, :not_found}
      doc -> {:ok, doc}
    end
  end

  defp load_content(doc) do
    case Storage.get(doc.content_hash) do
      {:ok, content} ->
        {:ok, doc, content}

      {:error, :not_found} ->
        # The row says the bytes are here and they are not. That is a real
        # inconsistency, not a missing file we already knew about, so it is
        # reported as its own thing rather than folded into :content_missing.
        {:error, :content_missing}
    end
  end

  defp uuid?(id) do
    match?({:ok, _}, Ecto.UUID.cast(id))
  end
end
