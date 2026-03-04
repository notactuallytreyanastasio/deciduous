defmodule Deciduex.Commands.DocTest do
  use ExUnit.Case

  import ExUnit.CaptureIO
  import Deciduex.TestFixtures

  alias Deciduex.Commands.Doc
  alias Deciduex.Queries

  @test_file_content "test content for document"
  @test_file_path "/tmp/deciduex_test_doc.txt"
  @test_docs_dir "/tmp/deciduex_test_docs"

  setup do
    create_tables!()

    insert_node!(%{
      id: 1,
      node_type: "goal",
      title: "Test Goal",
      created_at: "2024-01-01T10:00:00Z"
    })

    # Create test file
    File.write!(@test_file_path, @test_file_content)

    # Clean up test docs directory before each test
    File.rm_rf!(@test_docs_dir)

    # Set env var to use test docs directory
    System.put_env("DECIDUOUS_DOCS_PATH", @test_docs_dir)

    on_exit(fn ->
      File.rm(@test_file_path)
      File.rm_rf!(@test_docs_dir)
      System.delete_env("DECIDUOUS_DOCS_PATH")
    end)

    :ok
  end

  describe "doc attach" do
    test "attaches a file to a node" do
      output =
        capture_io(fn ->
          Doc.run(["attach", "1", @test_file_path])
        end)

      assert output =~ "Attached document"
      assert output =~ "node 1"
      assert output =~ "deciduex_test_doc.txt"

      # Verify document was created
      docs = Queries.list_documents()
      assert length(docs) == 1

      doc = hd(docs)
      assert doc.node_id == 1
      assert doc.original_filename == "deciduex_test_doc.txt"
      assert doc.mime_type == "text/plain"
      assert doc.file_size == byte_size(@test_file_content)
    end

    test "attaches with description" do
      output =
        capture_io(fn ->
          Doc.run(["attach", "1", @test_file_path, "-d", "My test document"])
        end)

      assert output =~ "Attached document"
      assert output =~ "Description: My test document"

      doc = Queries.list_documents() |> hd()
      assert doc.description == "My test document"
      assert doc.description_source == "manual"
    end

    test "creates documents directory if needed" do
      # Test docs dir should be created when attaching
      refute File.exists?(@test_docs_dir)

      capture_io(fn ->
        Doc.run(["attach", "1", @test_file_path])
      end)

      assert File.exists?(@test_docs_dir)
    end

    test "deduplicates files by content hash" do
      capture_io(fn ->
        Doc.run(["attach", "1", @test_file_path])
      end)

      # Create another file with same content
      other_path = "/tmp/deciduex_test_doc2.txt"
      File.write!(other_path, @test_file_content)

      capture_io(fn ->
        Doc.run(["attach", "1", other_path])
      end)

      File.rm(other_path)

      docs = Queries.list_documents()
      assert length(docs) == 2

      # Same content hash
      assert Enum.at(docs, 0).content_hash == Enum.at(docs, 1).content_hash
    end
  end

  describe "doc list" do
    setup do
      insert_document!(%{
        id: 1,
        node_id: 1,
        content_hash: "abc123",
        original_filename: "test.pdf",
        storage_filename: "test.pdf.abc12345",
        mime_type: "application/pdf",
        file_size: 1024,
        attached_at: "2024-01-01T12:00:00Z"
      })

      insert_document!(%{
        id: 2,
        node_id: 1,
        content_hash: "def456",
        original_filename: "image.png",
        storage_filename: "image.png.def45678",
        mime_type: "image/png",
        file_size: 2048,
        description: "Screenshot",
        description_source: "manual",
        attached_at: "2024-01-02T12:00:00Z"
      })

      :ok
    end

    test "lists all documents" do
      output =
        capture_io(fn ->
          Doc.run(["list"])
        end)

      assert output =~ "2 documents:"
      assert output =~ "test.pdf"
      assert output =~ "image.png"
      assert output =~ "Screenshot"
    end

    test "filters by node_id" do
      insert_node!(%{
        id: 2,
        node_type: "option",
        title: "Option 1",
        created_at: "2024-01-02T10:00:00Z"
      })

      insert_document!(%{
        id: 3,
        node_id: 2,
        content_hash: "ghi789",
        original_filename: "other.txt",
        storage_filename: "other.txt.ghi78901",
        mime_type: "text/plain",
        file_size: 100,
        attached_at: "2024-01-03T12:00:00Z"
      })

      output =
        capture_io(fn ->
          Doc.run(["list", "1"])
        end)

      assert output =~ "2 documents:"
      assert output =~ "test.pdf"
      refute output =~ "other.txt"
    end

    test "outputs json" do
      output =
        capture_io(fn ->
          Doc.run(["list", "--json"])
        end)

      data = Jason.decode!(output)
      assert length(data) == 2
      assert Enum.any?(data, &(&1["original_filename"] == "test.pdf"))
    end
  end

  describe "doc show" do
    setup do
      insert_document!(%{
        id: 1,
        node_id: 1,
        content_hash: "abc123def456789",
        original_filename: "test.pdf",
        storage_filename: "test.pdf.abc12345",
        mime_type: "application/pdf",
        file_size: 1024,
        description: "Test document",
        description_source: "manual",
        attached_at: "2024-01-01T12:00:00Z"
      })

      :ok
    end

    test "shows document details" do
      output =
        capture_io(fn ->
          Doc.run(["show", "1"])
        end)

      assert output =~ "Document Details"
      assert output =~ "ID:          1"
      assert output =~ "Node:        1"
      assert output =~ "Filename:    test.pdf"
      assert output =~ "MIME type:   application/pdf"
      assert output =~ "Size:        1 KB"
      assert output =~ "Hash:        abc123def456789"
      assert output =~ "Description: Test document (manual)"
    end

    test "outputs json" do
      output =
        capture_io(fn ->
          Doc.run(["show", "1", "--json"])
        end)

      data = Jason.decode!(output)
      assert data["id"] == 1
      assert data["original_filename"] == "test.pdf"
    end
  end

  describe "doc describe" do
    setup do
      insert_document!(%{
        id: 1,
        node_id: 1,
        content_hash: "abc123",
        original_filename: "test.pdf",
        storage_filename: "test.pdf.abc12345",
        mime_type: "application/pdf",
        file_size: 1024,
        attached_at: "2024-01-01T12:00:00Z"
      })

      :ok
    end

    test "updates description" do
      output =
        capture_io(fn ->
          Doc.run(["describe", "1", "Updated description"])
        end)

      assert output =~ "Updated description for document 1"

      doc = Queries.get_document(1)
      assert doc.description == "Updated description"
      assert doc.description_source == "manual"
    end
  end

  describe "doc detach" do
    setup do
      insert_document!(%{
        id: 1,
        node_id: 1,
        content_hash: "abc123",
        original_filename: "test.pdf",
        storage_filename: "test.pdf.abc12345",
        mime_type: "application/pdf",
        file_size: 1024,
        attached_at: "2024-01-01T12:00:00Z"
      })

      :ok
    end

    test "soft-deletes a document" do
      output =
        capture_io(fn ->
          Doc.run(["detach", "1"])
        end)

      assert output =~ "Detached document 1"

      # Document still exists but has detached_at set
      doc = Queries.get_document(1)
      assert doc.detached_at != nil

      # Not included in default list
      docs = Queries.list_documents()
      assert Enum.empty?(docs)

      # Included with --all flag
      docs_all = Queries.list_documents(nil, true)
      assert length(docs_all) == 1
    end
  end

  describe "doc gc" do
    test "dry run reports orphaned files" do
      File.mkdir_p!(@test_docs_dir)

      # Create orphan file
      orphan_path = Path.join(@test_docs_dir, "orphan.txt.abc12345")
      File.write!(orphan_path, "orphan content")

      output =
        capture_io(fn ->
          Doc.run(["gc", "--dry-run"])
        end)

      assert output =~ "Would remove 1 orphaned files"
      assert output =~ "orphan.txt.abc12345"

      # File still exists
      assert File.exists?(orphan_path)
    end

    test "removes orphaned files without --dry-run" do
      File.mkdir_p!(@test_docs_dir)

      # Create orphan file
      orphan_path = Path.join(@test_docs_dir, "orphan.txt.abc12345")
      File.write!(orphan_path, "orphan content")

      output =
        capture_io(fn ->
          Doc.run(["gc"])
        end)

      assert output =~ "Removing 1 orphaned files"
      assert output =~ "Removed: orphan.txt.abc12345"

      # File removed
      refute File.exists?(orphan_path)
    end
  end
end
