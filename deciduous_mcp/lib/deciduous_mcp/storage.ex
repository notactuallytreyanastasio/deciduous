defmodule DeciduousMcp.Storage do
  @moduledoc """
  Where document bytes live.

  One behaviour with one implementation today (`DeciduousMcp.Storage.Postgres`),
  so that moving blobs to object storage later is a new module and a config
  line rather than a change to every caller. Everything is keyed by content
  hash, which is what the CLI already names files by, so the same file attached
  in several projects is stored once.

  Postgres was chosen over object storage to start: the whole archive is 23MB
  with a 16MB largest file, which Postgres TOASTs without noticing, and it
  means a restore can never leave rows pointing at objects that a separate
  system lost. The cost is that `pg_dump` now carries the blobs, which matters
  when this grows — that is the point at which the S3 implementation earns
  itself.
  """

  @type hash :: String.t()

  @callback put(hash, binary, keyword) :: :ok | {:error, term}
  @callback get(hash) :: {:ok, binary} | {:error, :not_found | term}
  @callback exists?(hash) :: boolean
  @callback delete(hash) :: :ok | {:error, term}

  def backend do
    Application.get_env(:deciduous_mcp, :storage_backend, DeciduousMcp.Storage.Postgres)
  end

  def put(hash, content, opts \\ []), do: backend().put(hash, content, opts)
  def get(hash), do: backend().get(hash)
  def exists?(hash), do: backend().exists?(hash)
  def delete(hash), do: backend().delete(hash)

  @doc """
  The sha256 the CLI would have recorded for this content, lowercase hex.
  """
  def hash(content) when is_binary(content) do
    :crypto.hash(:sha256, content) |> Base.encode16(case: :lower)
  end

  @doc """
  Checks content against the hash it claims to have.

  Upload verifies rather than trusts: the hash is the primary key and the
  dedup key, so a client that sends the wrong one would quietly shadow another
  document's bytes for every project that references that hash.
  """
  def verify(hash, content) do
    actual = hash(content)

    if Plug.Crypto.secure_compare(String.downcase(hash), actual),
      do: :ok,
      else: {:error, {:hash_mismatch, expected: hash, actual: actual}}
  end
end
