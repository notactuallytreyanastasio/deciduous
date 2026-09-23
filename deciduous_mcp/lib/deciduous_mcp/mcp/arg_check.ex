defmodule DeciduousMcp.MCP.ArgCheck do
  @moduledoc """
  Holds a tool call's arguments to the JSON Schema the tool advertises,
  before the tool runs.

  Hermes validates arguments with Peri, and `DeciduousMcp.MCP.Component`
  derives the Peri schema from each tool's JSON Schema — but only the types
  of top-level strings, integers and booleans survive that translation.
  `enum`, `minimum`/`maximum`, lengths and everything inside arrays and
  objects were advertised to the client and then not checked at all:

      add_node   {"node_type": "feedback"}            stored
      add_node   {"status": "done"}                   stored
      add_node   {"files": "notalist"}                stored
      add_node   {"title": <1,000,000 characters>}    stored
      get_graph  {"max_nodes": 30000}                 over the documented 20000, served
      query_nodes {"limit": 9223372036854775808}      Postgrex.EncodeError and a stack trace

  This checks what the schema says, and two things no schema here says but
  every string must satisfy:

    * no NUL (U+0000). Postgres `text` and `jsonb` cannot store it; it came
      back as `%Postgrex.Error{code: :character_not_in_repertoire}` with the
      stack trace attached, from every tool that writes a string.
    * a size. `with_limits/1` gives every string property that does not
      declare a `maxLength` one (titles 10,000, branches 512, anything
      else 262,144 characters) and titles a `minLength` of 1, in the
      schema the client is sent as well as the one enforced here, so the
      two cannot disagree.

  The first violation is reported, naming the argument path and the value's
  offending property, never the whole value.
  """

  @title_max 10_000
  # A branch is a key: the write lock's, and the btree expression index
  # idx_nodes_ws_branchkey_latest over metadata->>'branch', whose entries
  # cannot pass about 2,700 bytes. 512 characters is at most 2,048 bytes of
  # UTF-8 and longer than any branch name a person types; a 300-character
  # one used to fail every write with MatchError (SERVER-N2).
  @branch_max 512
  @text_max 262_144
  # Every string in one call, keys included. Each string was bounded, the
  # call was not: update_node's metadata took 28 keys of 262,144 characters
  # and stored 7,340,032 characters on one node. 1 Mi is four full-length
  # descriptions, more than any one step of reasoning needs.
  @call_text_max 1_048_576
  # Arrays that do not declare maxItems. add_node files with 200,000 items
  # and capture_conversation_turn with 5,000 observations were written.
  @items_max 1_000

  # What does not count as content for a title: whitespace, separators,
  # controls, and format characters (U+200B zero width space, U+2060 word
  # joiner, U+FEFF byte order mark), which String.trim/1 keeps and which
  # render as nothing.
  @invisible "[\\s\\p{Z}\\p{Cc}\\p{Cf}]"
  @invisible_edges Regex.compile!("\\A#{@invisible}+|#{@invisible}+\\z", "u")

  @doc "Adds the default string bounds to a tool's input schema (atom or string keys)."
  def with_limits(%{} = schema) do
    schema
    |> maybe_update(:properties, fn props ->
      Map.new(props, fn {name, spec} -> {name, limit_property(to_string(name), spec)} end)
    end)
    |> maybe_update(:items, &with_limits/1)
  end

  def with_limits(other), do: other

  defp limit_property(name, %{} = spec) do
    spec = with_limits(spec)

    cond do
      string_type?(spec[:type]) and not Map.has_key?(spec, :maxLength) ->
        spec
        |> Map.put(:maxLength, default_max(name))
        |> then(fn s -> if name == "title", do: Map.put_new(s, :minLength, 1), else: s end)

      spec[:type] == "array" ->
        Map.put_new(spec, :maxItems, @items_max)

      true ->
        spec
    end
  end

  defp limit_property(_name, spec), do: spec

  defp default_max("title"), do: @title_max
  defp default_max("branch"), do: @branch_max
  defp default_max(_), do: @text_max

  defp string_type?("string"), do: true
  defp string_type?(types) when is_list(types), do: "string" in types
  defp string_type?(_), do: false

  defp maybe_update(map, key, fun) do
    if Map.has_key?(map, key), do: Map.update!(map, key, fun), else: map
  end

  @doc """
  `:ok`, or `{:error, message}` for the first argument that breaks the schema.
  `schema` uses atom keys, as the tools' `definition/0` maps do.
  """
  def check(schema, args) when is_map(args) do
    # Declared bounds first, so a title over its own limit is told that
    # limit rather than the general one.
    with :ok <- no_nul(args, "arguments"),
         :ok <- check_object(schema, args, nil),
         :ok <- no_oversized_string(args, "arguments") do
      within_call_limit(args)
    end
  end

  def check(_schema, args), do: {:error, "arguments must be an object, got #{describe(args)}"}

  # --- names ------------------------------------------------------------------

  # A caller's likely meaning for a name the tool does not have, most likely
  # first. Only the ones the tool declares are offered.
  @aliases %{
    "parent_id" => ~w(parent_node_id related_to),
    "parent" => ~w(parent_id parent_node_id related_to),
    "parent_node_id" => ~w(parent_id related_to),
    "related_to" => ~w(parent_id parent_node_id),
    "node_id" => ~w(parent_node_id goal_node_id related_to),
    "id" => ~w(node_id),
    "goal_id" => ~w(goal_node_id parent_node_id parent_id),
    "node_type" => ~w(type),
    "type" => ~w(node_type edge_type),
    "kind" => ~w(node_type type edge_type),
    "from" => ~w(from_node_id),
    "from_id" => ~w(from_node_id),
    "to" => ~w(to_node_id),
    "to_id" => ~w(to_node_id),
    "name" => ~w(title),
    "text" => ~w(description title),
    "content" => ~w(description),
    "body" => ~w(description),
    "reason" => ~w(rationale description),
    "query" => ~w(search question),
    "q" => ~w(search question),
    "chosen" => ~w(chosen_option),
    "options" => ~w(options_considered alternatives),
    "project" => ~w(workspace),
    "repo" => ~w(workspace)
  }

  @doc """
  `:ok`, or `{:error, message}` naming every argument the tool does not
  declare, each with the one it most likely meant.

  Hermes hands a tool only the keys its schema declares (Peri returns what
  it validated), so without this an unknown name did not fail: it vanished,
  and the call succeeded doing something other than what was asked.
  `args` must be the arguments as the client sent them.
  """
  def unknown_arguments(tool, schema, args) when is_map(args) do
    props = Map.get(schema, :properties, %{})
    declared = props |> Map.keys() |> Enum.map(&to_string/1)

    case args |> Map.keys() |> Enum.map(&to_string/1) |> Enum.reject(&(&1 in declared)) do
      [] ->
        :ok

      unknown ->
        named =
          unknown
          |> Enum.sort()
          |> Enum.map_join("; ", fn key ->
            case suggestions(key, props, declared) do
              [] -> inspect(key)
              meant -> "#{inspect(key)} (did you mean #{Enum.join(meant, " or ")}?)"
            end
          end)

        noun = if length(unknown) == 1, do: "argument", else: "arguments"

        {:error,
         "#{tool} has no #{noun} #{named}. Its arguments are: " <>
           Enum.join(Enum.sort(declared), ", ")}
    end
  end

  def unknown_arguments(_tool, _schema, _args), do: :ok

  defp suggestions(key, props, declared) do
    nested =
      case props[:metadata] || props["metadata"] do
        %{properties: meta} ->
          if Enum.any?(Map.keys(meta), &(to_string(&1) == key)), do: ["metadata.#{key}"], else: []

        _ ->
          []
      end

    aliased = Enum.filter(Map.get(@aliases, key, []), &(&1 in declared))

    near =
      declared
      |> Enum.map(&{&1, String.jaro_distance(key, &1)})
      |> Enum.filter(fn {_, d} -> d >= 0.85 end)
      |> Enum.sort_by(fn {_, d} -> -d end)
      |> Enum.map(&elem(&1, 0))

    Enum.uniq(nested ++ aliased ++ near) |> Enum.take(2)
  end

  # --- schema ---------------------------------------------------------------

  defp check_value(spec, value, path) do
    with :ok <- check_type(spec[:type], value, path),
         :ok <- check_enum(spec[:enum], value, path),
         :ok <- check_bounds(spec, value, path),
         :ok <- check_length(spec, value, path),
         :ok <- check_items(spec, value, path) do
      check_nested(spec, value, path)
    end
  end

  defp check_object(schema, map, path) do
    required = schema |> Map.get(:required, []) |> Enum.map(&to_string/1)
    props = Map.get(schema, :properties, %{})

    missing = Enum.find(required, fn key -> is_nil(Map.get(map, key)) end)

    if missing do
      {:error, "#{join(path, missing)} is required"}
    else
      Enum.reduce_while(props, :ok, fn {key, spec}, :ok ->
        key = to_string(key)

        case Map.get(map, key) do
          # A JSON null is how several clients say "not given"; every tool
          # already reads a missing argument and a null one the same way.
          nil ->
            {:cont, :ok}

          value ->
            case check_value(spec, value, join(path, key)) do
              :ok -> {:cont, :ok}
              error -> {:halt, error}
            end
        end
      end)
    end
  end

  defp check_nested(%{properties: _} = spec, value, path) when is_map(value),
    do: check_object(spec, value, path)

  defp check_nested(_spec, _value, _path), do: :ok

  defp check_type(nil, _value, _path), do: :ok

  defp check_type(types, value, path) when is_list(types) do
    if Enum.any?(types, &type?(&1, value)),
      do: :ok,
      else: {:error, "#{path} must be #{Enum.join(types, " or ")}, got #{describe(value)}"}
  end

  defp check_type(type, value, path) do
    if type?(type, value),
      do: :ok,
      else: {:error, "#{path} must be #{article(type)} #{type}, got #{describe(value)}"}
  end

  defp type?("string", v), do: is_binary(v)
  defp type?("integer", v), do: is_integer(v)
  defp type?("number", v), do: is_number(v)
  defp type?("boolean", v), do: is_boolean(v)
  defp type?("array", v), do: is_list(v)
  defp type?("object", v), do: is_map(v)
  defp type?(_, _), do: true

  defp check_enum(nil, _value, _path), do: :ok

  defp check_enum(allowed, value, path) do
    if value in allowed,
      do: :ok,
      else: {:error, "#{path} must be one of #{Enum.join(allowed, ", ")}; got #{describe(value)}"}
  end

  defp check_bounds(spec, value, path) when is_number(value) do
    cond do
      is_number(spec[:minimum]) and value < spec[:minimum] ->
        {:error, "#{path} must be at least #{spec[:minimum]}, got #{value}"}

      is_number(spec[:maximum]) and value > spec[:maximum] ->
        {:error, "#{path} must be at most #{spec[:maximum]}, got #{value}"}

      true ->
        :ok
    end
  end

  defp check_bounds(_spec, _value, _path), do: :ok

  defp check_length(spec, value, path) when is_binary(value) do
    length = String.length(value)

    cond do
      is_integer(spec[:minLength]) and
          String.length(Regex.replace(@invisible_edges, value, "")) < spec[:minLength] ->
        {:error, "#{path} must not be blank"}

      is_integer(spec[:maxLength]) and length > spec[:maxLength] ->
        {:error, "#{path} is #{length} characters; the limit is #{spec[:maxLength]}"}

      true ->
        :ok
    end
  end

  defp check_length(_spec, _value, _path), do: :ok

  defp check_items(spec, list, path) when is_list(list) do
    max = spec[:maxItems]

    cond do
      is_integer(max) and length(list) > max ->
        {:error, "#{path} has #{length(list)} items; the limit is #{max}"}

      is_map(spec[:items]) ->
        list
        |> Enum.with_index()
        |> Enum.reduce_while(:ok, fn {item, i}, :ok ->
          case check_value(spec[:items], item, "#{path}[#{i}]") do
            :ok -> {:cont, :ok}
            error -> {:halt, error}
          end
        end)

      true ->
        :ok
    end
  end

  defp check_items(_spec, _value, _path), do: :ok

  # --- every string, declared or not ----------------------------------------

  defp no_nul(value, path) when is_binary(value) do
    if String.contains?(value, <<0>>),
      do:
        {:error,
         "#{path} contains a NUL character (U+0000), which cannot be stored; remove it and retry"},
      else: :ok
  end

  defp no_nul(%{} = map, path), do: each(map, path, &no_nul/2, true)
  defp no_nul(list, path) when is_list(list), do: each_list(list, path, &no_nul/2)
  defp no_nul(_value, _path), do: :ok

  # Undeclared fields (update_node's free-form metadata, unknown keys) get
  # the same ceiling as declared text, so no string anywhere is unbounded.
  defp no_oversized_string(value, path) when is_binary(value) do
    length = String.length(value)

    if length > @text_max,
      do: {:error, "#{path} is #{length} characters; the limit is #{@text_max}"},
      else: :ok
  end

  defp no_oversized_string(%{} = map, path), do: each(map, path, &no_oversized_string/2, true)

  defp no_oversized_string(list, path) when is_list(list),
    do: each_list(list, path, &no_oversized_string/2)

  defp no_oversized_string(_value, _path), do: :ok

  defp within_call_limit(args) do
    total = text_size(args)

    if total > @call_text_max,
      do:
        {:error,
         "the arguments hold #{total} characters of text in all (largest: #{largest_key(args)}); " <>
           "the limit for one call is #{@call_text_max}"},
      else: :ok
  end

  defp text_size(v) when is_binary(v), do: String.length(v)

  defp text_size(%{} = map),
    do: Enum.reduce(map, 0, fn {k, v}, acc -> acc + text_size(to_string(k)) + text_size(v) end)

  defp text_size(list) when is_list(list), do: Enum.reduce(list, 0, &(text_size(&1) + &2))
  defp text_size(_), do: 0

  defp largest_key(args) do
    {key, _} = Enum.max_by(args, fn {_k, v} -> text_size(v) end)
    key
  end

  defp each(map, path, fun, check_keys?) do
    Enum.reduce_while(map, :ok, fn {k, v}, :ok ->
      key_result = if check_keys? and is_binary(k), do: fun.(k, "#{path} key"), else: :ok

      case key_result do
        :ok ->
          case fun.(v, join(path, to_string(k))) do
            :ok -> {:cont, :ok}
            error -> {:halt, error}
          end

        error ->
          {:halt, error}
      end
    end)
  end

  defp each_list(list, path, fun) do
    list
    |> Enum.with_index()
    |> Enum.reduce_while(:ok, fn {v, i}, :ok ->
      case fun.(v, "#{path}[#{i}]") do
        :ok -> {:cont, :ok}
        error -> {:halt, error}
      end
    end)
  end

  # --- wording ----------------------------------------------------------------

  defp join(nil, key), do: key
  defp join("arguments", key), do: key
  defp join(path, key), do: "#{path}.#{key}"

  defp article(type) when type in ["integer", "array", "object"], do: "an"
  defp article(_), do: "a"

  # Enough of the value to recognise it, never all of it: the value may be
  # the 7 MB string that is being refused.
  defp describe(v) when is_binary(v) and byte_size(v) > 60,
    do: inspect(String.slice(v, 0, 40) <> "...", binaries: :as_strings)

  defp describe(v) when is_binary(v), do: inspect(v, binaries: :as_strings)
  defp describe(v) when is_list(v), do: "an array"
  defp describe(v) when is_map(v), do: "an object"
  defp describe(nil), do: "null"
  defp describe(v), do: inspect(v)
end
