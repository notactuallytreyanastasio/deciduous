defmodule Deciduex.Commands.Doc do
  @moduledoc """
  Implements the `doc` command with subcommands for document management.

  Subcommands:
    attach <node_id> <file> [-d description]  Attach file to a node
    list [node_id] [--json] [--all]           List documents
    show <doc_id> [--json]                    Show document details
    describe <doc_id> [description]           Update description (stdin if no arg)
    open <doc_id>                             Open document in default app
    detach <doc_id>                           Soft-delete document
    gc [--dry-run]                            Remove orphaned files
  """

  alias Deciduex.DB
  alias Deciduex.Mutations
  alias Deciduex.Queries

  @mime_types %{
    ".png" => "image/png",
    ".jpg" => "image/jpeg",
    ".jpeg" => "image/jpeg",
    ".gif" => "image/gif",
    ".webp" => "image/webp",
    ".svg" => "image/svg+xml",
    ".pdf" => "application/pdf",
    ".md" => "text/markdown",
    ".txt" => "text/plain",
    ".json" => "application/json",
    ".yaml" => "text/yaml",
    ".yml" => "text/yaml",
    ".html" => "text/html",
    ".css" => "text/css",
    ".js" => "text/javascript"
  }

  def run(["attach" | rest]), do: attach(rest)
  def run(["list" | rest]), do: list(rest)
  def run(["show" | rest]), do: show(rest)
  def run(["describe" | rest]), do: describe(rest)
  def run(["open" | rest]), do: open(rest)
  def run(["detach" | rest]), do: detach(rest)
  def run(["gc" | rest]), do: gc(rest)
  def run([]), do: print_usage()
  def run([unknown | _]), do: error("Unknown doc subcommand: #{unknown}")

  # Attach subcommand

  defp attach(args) do
    with {:ok, node_id, file_path, description} <- parse_attach_args(args),
         :ok <- validate_file_exists(file_path),
         {:ok, node} <- fetch_node(node_id) do
      do_attach(node, file_path, description)
    else
      {:error, reason} -> error(reason)
    end
  end

  defp do_attach(node, file_path, description) do
    {:ok, file_bytes} = File.read(file_path)
    hash = compute_hash(file_bytes)
    original_filename = Path.basename(file_path)
    storage_filename = "#{original_filename}.#{String.slice(hash, 0, 8)}"

    store_file(file_bytes, storage_filename)
    insert_document(node, hash, original_filename, storage_filename, file_bytes, description)
  end

  defp insert_document(node, hash, original_filename, storage_filename, file_bytes, description) do
    attrs = build_document_attrs(node, hash, original_filename, storage_filename, file_bytes, description)

    case Mutations.attach_document(attrs) do
      {:ok, id} ->
        print_attach_success(id, node.id, original_filename, description)
        Mutations.log_command("doc attach", [to_string(node.id)], 0)

      {:error, reason} ->
        error("Failed to attach: #{inspect(reason)}")
    end
  end

  defp build_document_attrs(node, hash, original_filename, storage_filename, file_bytes, description) do
    %{
      node_id: node.id,
      node_change_id: node.change_id,
      content_hash: hash,
      original_filename: original_filename,
      storage_filename: storage_filename,
      mime_type: detect_mime_type(original_filename),
      file_size: byte_size(file_bytes),
      description: description,
      description_source: description_source(description)
    }
  end

  defp description_source(nil), do: "none"
  defp description_source(_), do: "manual"

  defp print_attach_success(id, node_id, filename, nil) do
    IO.puts("Attached document #{id} to node #{node_id} (#{filename})")
  end

  defp print_attach_success(id, node_id, filename, description) do
    IO.puts("Attached document #{id} to node #{node_id} (#{filename})")
    IO.puts("  Description: #{truncate(description, 80)}")
  end

  # List subcommand

  defp list(args) do
    {node_id, json?, include_detached?} = parse_list_args(args)
    docs = Queries.list_documents(node_id, include_detached?)

    if json? do
      IO.puts(Jason.encode!(docs, pretty: true))
    else
      print_document_list(docs)
    end
  end

  defp print_document_list([]), do: IO.puts("No documents found.")

  defp print_document_list(docs) do
    IO.puts("#{length(docs)} documents:")
    print_header()
    Enum.each(docs, &print_document_row/1)
  end

  defp print_header do
    IO.puts(
      String.pad_trailing("ID", 5) <>
        String.pad_trailing("NODE", 8) <>
        String.pad_trailing("FILENAME", 25) <>
        String.pad_trailing("TYPE", 10) <>
        String.pad_trailing("SIZE", 8) <>
        "DESCRIPTION"
    )

    IO.puts(String.duplicate("-", 80))
  end

  defp print_document_row(d) do
    IO.puts(
      String.pad_trailing(to_string(d.id), 5) <>
        String.pad_trailing(to_string(d.node_id), 8) <>
        String.pad_trailing(truncate(d.original_filename, 24), 25) <>
        String.pad_trailing(truncate(d.mime_type, 9), 10) <>
        String.pad_trailing(format_file_size(d.file_size), 8) <>
        truncate(d.description || "", 30)
    )
  end

  # Show subcommand

  defp show(args) do
    with {:ok, doc_id, json?} <- parse_show_args(args),
         {:ok, doc} <- fetch_document(doc_id) do
      if json? do
        IO.puts(Jason.encode!(doc, pretty: true))
      else
        print_document_details(doc)
      end
    else
      {:error, reason} -> error(reason)
    end
  end

  defp print_document_details(doc) do
    IO.puts("Document Details")
    IO.puts("  ID:          #{doc.id}")
    IO.puts("  Node:        #{doc.node_id}")
    IO.puts("  Filename:    #{doc.original_filename}")
    IO.puts("  MIME type:   #{doc.mime_type}")
    IO.puts("  Size:        #{format_file_size(doc.file_size)}")
    IO.puts("  Hash:        #{doc.content_hash}")
    IO.puts("  Storage:     .deciduous/documents/#{doc.storage_filename}")
    IO.puts("  Attached:    #{doc.attached_at}")
    if doc.attached_by, do: IO.puts("  Attached by: #{doc.attached_by}")
    if doc.description, do: IO.puts("  Description: #{doc.description} (#{doc.description_source})")
    if doc.detached_at, do: IO.puts("  DETACHED")
  end

  # Describe subcommand

  defp describe(args) do
    with {:ok, doc_id, description} <- parse_describe_args(args),
         {:ok, _doc} <- fetch_document(doc_id),
         :ok <- Mutations.update_document_description(doc_id, description, "manual") do
      IO.puts("Updated description for document #{doc_id}")
      Mutations.log_command("doc describe", [to_string(doc_id)], 0)
    else
      {:error, :not_found} -> error("Document not found")
      {:error, reason} -> error(to_string(reason))
    end
  end

  # Open subcommand

  defp open(args) do
    with {:ok, doc_id} <- parse_id_arg(args, "open"),
         {:ok, doc} <- fetch_document(doc_id),
         :ok <- open_document_file(doc) do
      IO.puts("Opened #{doc.original_filename}")
    else
      {:error, reason} -> error(to_string(reason))
    end
  end

  defp open_document_file(doc) do
    file_path = Path.join(get_documents_dir(), doc.storage_filename)

    if File.exists?(file_path) do
      open_file_in_app(file_path, doc.original_filename)
    else
      {:error, "File not found on disk: #{file_path}"}
    end
  end

  defp open_file_in_app(file_path, original_filename) do
    temp_dir = Path.join(System.tmp_dir!(), "deciduous-docs")
    File.mkdir_p!(temp_dir)
    temp_path = Path.join(temp_dir, original_filename)
    File.cp!(file_path, temp_path)

    open_cmd = if :os.type() == {:unix, :darwin}, do: "open", else: "xdg-open"

    case System.cmd(open_cmd, [temp_path]) do
      {_, 0} -> :ok
      {_, _} -> {:error, "Failed to open file"}
    end
  end

  # Detach subcommand

  defp detach(args) do
    with {:ok, doc_id} <- parse_id_arg(args, "detach"),
         :ok <- Mutations.detach_document(doc_id) do
      IO.puts("Detached document #{doc_id}")
      Mutations.log_command("doc detach", [to_string(doc_id)], 0)
    else
      {:error, :not_found} -> error("Document not found")
      {:error, reason} -> error(to_string(reason))
    end
  end

  # GC subcommand

  defp gc(args) do
    dry_run? = "--dry-run" in args
    docs_dir = get_documents_dir()

    if File.exists?(docs_dir) do
      run_gc(docs_dir, dry_run?)
    else
      IO.puts("No documents directory found.")
    end
  end

  defp run_gc(docs_dir, dry_run?) do
    active_hashes = Queries.get_active_content_hashes() |> MapSet.new()
    {:ok, files} = File.ls(docs_dir)
    orphans = find_orphan_files(files, active_hashes)

    if Enum.empty?(orphans) do
      IO.puts("No orphaned files found.")
    else
      remove_orphans(docs_dir, orphans, dry_run?)
    end
  end

  defp find_orphan_files(files, active_hashes) do
    Enum.filter(files, fn filename ->
      case Regex.run(~r/\.([a-f0-9]{8})$/, filename) do
        [_, hash_prefix] -> not Enum.any?(active_hashes, &String.starts_with?(&1, hash_prefix))
        nil -> true
      end
    end)
  end

  defp remove_orphans(docs_dir, orphans, dry_run?) do
    action = if dry_run?, do: "Would remove", else: "Removing"
    IO.puts("#{action} #{length(orphans)} orphaned files:")

    total_size =
      Enum.reduce(orphans, 0, fn filename, acc ->
        path = Path.join(docs_dir, filename)
        {:ok, stat} = File.stat(path)
        size = stat.size

        if dry_run? do
          IO.puts("  #{filename} (#{format_file_size(size)})")
        else
          File.rm!(path)
          IO.puts("  Removed: #{filename} (#{format_file_size(size)})")
        end

        acc + size
      end)

    IO.puts("\nTotal: #{format_file_size(total_size)}")

    unless dry_run? do
      Mutations.log_command("doc gc", [], 0)
    end
  end

  # Argument parsing helpers

  defp parse_attach_args([node_id_str, file_path | rest]) do
    case Integer.parse(node_id_str) do
      {node_id, ""} -> {:ok, node_id, file_path, parse_description_flag(rest)}
      _ -> {:error, "Invalid node ID: #{node_id_str}"}
    end
  end

  defp parse_attach_args(_), do: {:error, "Usage: doc attach <node_id> <file> [-d description]"}

  defp parse_description_flag(["-d", desc | _]), do: desc
  defp parse_description_flag(["--description", desc | _]), do: desc
  defp parse_description_flag(_), do: nil

  defp parse_list_args(args) do
    json? = "--json" in args
    include_detached? = "--all" in args

    node_id =
      args
      |> Enum.reject(&(&1 in ["--json", "--all"]))
      |> List.first()
      |> parse_optional_int()

    {node_id, json?, include_detached?}
  end

  defp parse_show_args(args) do
    json? = "--json" in args

    case args |> Enum.reject(&(&1 == "--json")) do
      [doc_id_str | _] -> parse_id_result(doc_id_str, json?)
      [] -> {:error, "Usage: doc show <doc_id> [--json]"}
    end
  end

  defp parse_id_result(id_str, extra) do
    case Integer.parse(id_str) do
      {id, ""} -> {:ok, id, extra}
      _ -> {:error, "Invalid document ID: #{id_str}"}
    end
  end

  defp parse_describe_args([doc_id_str | rest]) do
    case Integer.parse(doc_id_str) do
      {doc_id, ""} ->
        description = get_description_value(rest)
        {:ok, doc_id, description}

      _ ->
        {:error, "Invalid document ID: #{doc_id_str}"}
    end
  end

  defp parse_describe_args([]), do: {:error, "Usage: doc describe <doc_id> [description]"}

  defp get_description_value([]), do: IO.read(:stdio, :all) |> String.trim()
  defp get_description_value([desc | _]), do: desc

  defp parse_id_arg([id_str | _], _cmd) do
    case Integer.parse(id_str) do
      {id, ""} -> {:ok, id}
      _ -> {:error, "Invalid document ID: #{id_str}"}
    end
  end

  defp parse_id_arg([], cmd), do: {:error, "Usage: doc #{cmd} <doc_id>"}

  defp parse_optional_int(nil), do: nil

  defp parse_optional_int(str) do
    case Integer.parse(str) do
      {n, ""} -> n
      _ -> nil
    end
  end

  # Validation helpers

  defp validate_file_exists(path) do
    if File.exists?(path), do: :ok, else: {:error, "File not found: #{path}"}
  end

  defp fetch_node(id) do
    case Queries.get_node(id) do
      nil -> {:error, "Node ##{id} not found"}
      node -> {:ok, node}
    end
  end

  defp fetch_document(id) do
    case Queries.get_document(id) do
      nil -> {:error, "Document #{id} not found"}
      doc -> {:ok, doc}
    end
  end

  # Utility helpers

  defp compute_hash(bytes) do
    :crypto.hash(:sha256, bytes) |> Base.encode16(case: :lower)
  end

  defp store_file(file_bytes, storage_filename) do
    docs_dir = get_documents_dir()
    File.mkdir_p!(docs_dir)
    dest_path = Path.join(docs_dir, storage_filename)
    unless File.exists?(dest_path), do: File.write!(dest_path, file_bytes)
  end

  defp get_documents_dir do
    case System.get_env("DECIDUOUS_DOCS_PATH") do
      nil -> get_default_documents_dir()
      path -> path
    end
  end

  defp get_default_documents_dir do
    case DB.find_db_path() do
      {:ok, db_path} -> db_path |> Path.dirname() |> Path.join("documents")
      :error -> ".deciduous/documents"
    end
  end

  defp detect_mime_type(filename) do
    ext = filename |> Path.extname() |> String.downcase()
    Map.get(@mime_types, ext, "application/octet-stream")
  end

  defp format_file_size(bytes) when bytes < 1024, do: "#{bytes} B"
  defp format_file_size(bytes) when bytes < 1024 * 1024, do: "#{div(bytes, 1024)} KB"
  defp format_file_size(bytes), do: "#{Float.round(bytes / (1024 * 1024), 1)} MB"

  defp truncate(str, max_len) when byte_size(str) <= max_len, do: str
  defp truncate(str, max_len), do: String.slice(str, 0, max_len - 3) <> "..."

  defp error(msg) do
    IO.puts(:stderr, "Error: #{msg}")
    System.halt(1)
  end

  defp print_usage do
    IO.puts("""
    Usage: deciduex doc <subcommand> [options]

    Subcommands:
      attach <node_id> <file> [-d desc]  Attach file to a node
      list [node_id] [--json] [--all]    List documents
      show <doc_id> [--json]             Show document details
      describe <doc_id> [description]    Update description
      open <doc_id>                      Open in default app
      detach <doc_id>                    Soft-delete document
      gc [--dry-run]                     Remove orphaned files
    """)
  end
end
