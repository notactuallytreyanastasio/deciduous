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
  """
  def fetch(id) do
    with {:ok, doc} <- lookup(id) do
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

  defp lookup(id) do
    result =
      if uuid?(id) do
        Repo.get(Document, id)
      else
        Repo.one(
          from d in Document,
            where: d.content_hash == ^String.downcase(id),
            order_by: [asc: d.content_missing, asc: d.inserted_at],
            limit: 1
        )
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
