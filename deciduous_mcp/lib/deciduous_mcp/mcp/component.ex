defmodule DeciduousMcp.MCP.Component do
  @moduledoc """
  Adapts this codebase's `definition/0` + `call/1` components onto the Hermes
  0.14 component behaviours.

  Every tool here was written against an API Hermes does not have. Hermes'
  own `use Hermes.Server.Component` generates `input_schema/0` from a
  `schema do ... end` block written in Peri's DSL, and dispatches to
  `execute/2` for tools or `get_messages/2` for prompts. These modules instead
  hand-roll a `definition/0` map holding name, description and a literal JSON
  Schema, and answer on `call/1`. The result did not compile: every component
  raised `undefined function __mcp_raw_schema__/0` at the `use` line.

  The `definition/0` maps are good — they carry hand-written JSON Schema with
  enums and descriptions that a Peri schema would not express as precisely —
  so this adapts them rather than rewriting seventeen files into a DSL.

  What it supplies:

    * `__mcp_component_type__/0`, which is the whole of Hermes' definition of
      "is a component" (`Component.component?/1` calls it).
    * `__description__/0` from `definition().description` rather than the
      module's `@moduledoc`, which is what Hermes would otherwise advertise.
    * `input_schema/0` / `arguments/0`, with atom keys converted to strings —
      Hermes hands the schema straight to Jason, and `%{type: "object"}`
      encodes as `{"type":"object"}` either way, but the MCP clients compare
      key names, so they are normalized once here.
    * `execute/2` and `get_messages/2`, which call `call/1` and translate its
      `{:ok, json}` / `{:error, %{message: _}}` into what Hermes expects.
  """

  alias Hermes.MCP.Error
  alias Hermes.Server.Response

  require Logger

  defmacro __using__(type: :tool) do
    quote do
      @behaviour Hermes.Server.Component.Tool

      import Hermes.Server.Frame

      @doc false
      def __mcp_component_type__, do: :tool

      @doc false
      def __description__, do: definition()[:description]

      @impl Hermes.Server.Component.Tool
      def input_schema do
        definition()[:input_schema]
        |> DeciduousMcp.MCP.ArgCheck.with_limits()
        |> DeciduousMcp.MCP.Component.stringify()
      end

      @doc false
      def __mcp_raw_schema__ do
        DeciduousMcp.MCP.Component.peri_schema(definition()[:input_schema])
      end

      @impl Hermes.Server.Component.Tool
      def execute(params, frame) do
        DeciduousMcp.MCP.Component.dispatch_tool(__MODULE__, params, frame)
      end

      defoverridable execute: 2, input_schema: 0
    end
  end

  defmacro __using__(type: :prompt) do
    quote do
      @behaviour Hermes.Server.Component.Prompt

      import Hermes.Server.Frame

      @doc false
      def __mcp_component_type__, do: :prompt

      @doc false
      def __description__, do: definition()[:description]

      @doc false
      def __mcp_raw_schema__ do
        DeciduousMcp.MCP.Component.peri_schema_from_arguments(definition()[:arguments])
      end

      @impl Hermes.Server.Component.Prompt
      def arguments do
        definition()
        |> Map.get(:arguments, [])
        |> Enum.map(&DeciduousMcp.MCP.Component.stringify/1)
      end

      @impl Hermes.Server.Component.Prompt
      def get_messages(args, frame) do
        DeciduousMcp.MCP.Component.dispatch_prompt(__MODULE__, args, frame)
      end

      defoverridable get_messages: 2, arguments: 0
    end
  end

  @doc """
  Invokes a tool's `call/1` and translates the result for Hermes.

  Dispatch goes through this function, with the module as a variable, rather
  than being inlined into each component. Several tools can only ever return
  `{:ok, _}`, and when the call is inlined Elixir's type inference sees that
  and reports the `{:error, _}` clauses as unreachable — one warning per such
  module, for a clause the macro genuinely needs for the tools that do fail.
  """
  # Every argument that names a node. A value here that is not a UUID reached
  # Ecto as `where n.id == ^""`, which raises Ecto.Query.CastError inside the
  # handler; on production (2026-09-22 14:18 and 15:01) that took the Hermes
  # server process down and every session with it. Handlers now run in their
  # own task, so the same input is a contained error, but it is still a
  # crash and still a stack trace where a one-line answer belongs.
  # `goal_node_id` (close_thread) was missing: "PLACEHOLDER_SKIP" there
  # crashed with Ecto.CastError after the outcome had been written.
  @id_keys ~w(node_id from_node_id to_node_id related_to took_from parent_node_id parent_id goal_node_id)

  def dispatch_tool(module, params, frame) do
    params = params || %{}

    case invalid_id(params) do
      {key, value} ->
        {:error, Error.execution("#{key} is not a node id: #{inspect(value)}#{id_hint(key)}"),
         frame}

      nil ->
        schema = DeciduousMcp.MCP.ArgCheck.with_limits(module.definition()[:input_schema] || %{})

        case DeciduousMcp.MCP.ArgCheck.check(schema, params) do
          :ok ->
            dispatch_valid_tool(module, params, frame)

          # Answered like every other refusal a tool makes (execution error,
          # the sentence as the message), not as -32602 with the sentence
          # tucked into `data`: the message is what a client shows the model.
          # The suffix is true for reads too, and it is the thing a caller
          # retrying a write most needs to know.
          {:error, message} ->
            {:error, Error.execution(message <> "; nothing was written"), frame}
        end
    end
  end

  defp invalid_id(params) do
    Enum.find_value(@id_keys, fn key ->
      case Map.get(params, key) do
        nil ->
          nil

        value when is_binary(value) ->
          if uuid_text?(value), do: nil, else: {key, value}

        value ->
          {key, value}
      end
    end)
  end

  # The usual way to get here: add_node and add_edge sent in one batch, the
  # edge carrying a placeholder for an id that did not exist yet.
  defp id_hint(key) when key in ["from_node_id", "to_node_id"],
    do:
      ". To link a node you are creating now, pass parent_id to add_node instead; " <>
        "an id must come from a previous answer, not be written ahead of it"

  defp id_hint(_key), do: ""

  # Only the 36-character text form. Ecto.UUID.cast/1 also accepts any
  # 16-byte binary as a raw UUID, so "PLACEHOLDER_SKIP" passed this guard and
  # crashed in the query (production, 2026-09-23 02:37).
  defp uuid_text?(value) do
    byte_size(value) == 36 and match?({:ok, _}, Ecto.UUID.cast(value))
  end

  # The try is the last line of defence, not the handling: a tool that can
  # fail says why itself. Without it, an exception reached Hermes, which
  # answers "request handler crashed" with the inspected exception and stack
  # trace in `data` — for a Postgres error that is the Postgrex struct, and
  # for a failed insert the row being written. The client gets one line
  # naming the tool and the exception's type; the log gets the rest.
  defp dispatch_valid_tool(module, params, frame) do
    module.call(%{arguments: params, server: frame})
  rescue
    exception ->
      crashed(
        module,
        frame,
        Exception.format(:error, exception, __STACKTRACE__),
        exception.__struct__
      )
  catch
    kind, reason ->
      crashed(module, frame, Exception.format(kind, reason, __STACKTRACE__), kind)
  else
    result -> translate_tool_result(result, frame)
  end

  defp crashed(module, frame, formatted, what) do
    name = module.definition()[:name]
    Logger.error("tool #{name} crashed: " <> formatted)

    {:error,
     Error.execution(
       "#{name} failed (#{inspect(what)}); nothing it had not committed was kept, " <>
         "and the details are in the server log"
     ), frame}
  end

  defp translate_tool_result(result, frame) do
    case result do
      {:ok, payload} when is_binary(payload) ->
        {:reply, Response.text(Response.tool(), payload), frame}

      {:ok, payload} ->
        {:reply, Response.json(Response.tool(), payload), frame}

      {:error, %{message: message}} ->
        {:error, Error.execution(message), frame}

      {:error, other} ->
        {:error, Error.execution(describe_error(other)), frame}
    end
  end

  @doc """
  Invokes a prompt's `call/1` and unwraps the `messages` list Hermes expects.
  """
  def dispatch_prompt(module, args, frame) do
    case module.call(%{arguments: args || %{}, server: frame}) do
      {:ok, %{messages: messages}} -> {:reply, messages, frame}
      {:ok, other} -> {:reply, other, frame}
      {:error, %{message: message}} -> {:error, Error.execution(message), frame}
      {:error, other} -> {:error, Error.execution(describe_error(other)), frame}
    end
  end

  @doc """
  Derives a Peri schema from a tool's JSON Schema `properties`.

  Hermes builds each tool's `validate_input` as
  `__mcp_raw_schema__() |> __clean_schema_for_peri__() |> Peri.validate(params)`,
  and the call handler forwards **Peri's output**, not the original params:

      with {:error, errors} <- tool.validate_input.(params)

  An `{:ok, cleaned}` falls straight through that `with`. Peri returns only the
  keys its schema declares, so a stub `%{}` schema type-checks everything and
  hands every tool an empty argument map — `Peri.validate(%{}, %{"workspace" =>
  "deciduous"})` is `{:ok, %{}}`. Every argument would vanish silently, which is
  worse than not validating at all. So the schema is generated from the
  properties each tool already documents.

  Types map loosely on purpose. `number`, `array` and `object` become `:any`
  rather than a precise Peri type: a wrong guess here does not surface as a
  schema bug, it surfaces as a tool rejecting valid input.
  """
  def peri_schema(%{properties: properties} = schema) do
    required = schema |> Map.get(:required, []) |> Enum.map(&to_string/1) |> MapSet.new()

    Map.new(properties, fn {key, spec} ->
      name = to_string(key)
      type = peri_type(spec[:type] || spec["type"])
      {name, if(MapSet.member?(required, name), do: {:required, type}, else: type)}
    end)
  end

  def peri_schema(_), do: %{}

  @doc false
  def peri_schema_from_arguments(arguments) when is_list(arguments) do
    Map.new(arguments, fn arg ->
      name = to_string(arg[:name] || arg["name"])
      if arg[:required] || arg["required"], do: {name, {:required, :any}}, else: {name, :any}
    end)
  end

  def peri_schema_from_arguments(_), do: %{}

  defp peri_type("string"), do: :string
  defp peri_type("integer"), do: :integer
  defp peri_type("boolean"), do: :boolean
  defp peri_type(_), do: :any

  @doc """
  Deep-converts atom keys to strings so a schema written with Elixir atom keys
  is advertised with the key names MCP clients expect.
  """
  def stringify(%{} = map) do
    Map.new(map, fn {k, v} -> {to_string_key(k), stringify(v)} end)
  end

  def stringify(list) when is_list(list), do: Enum.map(list, &stringify/1)
  def stringify(other), do: other

  defp to_string_key(k) when is_atom(k), do: Atom.to_string(k)
  defp to_string_key(k), do: k

  @doc """
  One sentence for a tool failure, without the internals.

  An `Ecto.Changeset` used to be `inspect`ed into the message, which prints
  the changes being written and the struct they were written to; the
  client only needs the validation errors, field by field. Anything this
  does not recognise is logged in full and answered with its shape only.
  """
  def describe_error(reason) when is_binary(reason), do: reason
  def describe_error(reason) when is_atom(reason), do: to_string(reason)

  def describe_error(%Ecto.Changeset{} = changeset) do
    changeset
    |> Ecto.Changeset.traverse_errors(fn {message, opts} ->
      Enum.reduce(opts, message, fn {key, value}, acc ->
        String.replace(acc, "%{#{key}}", to_string_safe(value))
      end)
    end)
    |> Enum.map_join("; ", fn {field, messages} -> "#{field}: #{Enum.join(messages, ", ")}" end)
  end

  def describe_error({:node_not_found, id}), do: "#{id} is not a node in this workspace"

  def describe_error(reason) do
    Logger.error("unrecognised tool error: " <> inspect(reason, limit: :infinity))
    "unexpected error (#{error_shape(reason)}); the details are in the server log"
  end

  defp error_shape(reason) when is_tuple(reason) and tuple_size(reason) > 0,
    do: "tuple starting #{inspect(elem(reason, 0), limit: 3)}"

  defp error_shape(%{__struct__: struct}), do: inspect(struct)
  defp error_shape(reason) when is_map(reason), do: "map"
  defp error_shape(reason) when is_list(reason), do: "list"
  defp error_shape(_), do: "term"

  defp to_string_safe(value) when is_binary(value) or is_number(value) or is_atom(value),
    do: to_string(value)

  defp to_string_safe(value), do: inspect(value, limit: 5)
end
