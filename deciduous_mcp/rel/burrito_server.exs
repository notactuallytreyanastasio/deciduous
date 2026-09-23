defmodule DeciduousMcp.BurritoServer do
  @moduledoc false

  # Burrito 1.6 starts Elixir's CLI after starting the release. Without
  # --no-halt, Kernel.CLI resets System.no_halt(false) and exits even when
  # the application has a running supervisor. Burrito has no default-argv
  # option, so adapt its launcher in a disposable compiler source tree.
  # The fetched Hex dependency stays intact and remains auditable.
  def wrap(release) do
    source = Path.join(Mix.Project.deps_path(), "burrito")
    suffix = :crypto.strong_rand_bytes(8) |> Base.encode16(case: :lower)
    build_source = Path.join(System.tmp_dir!(), "deciduous_burrito_#{suffix}")
    File.mkdir!(build_source)

    try do
      for file <- ["src", "build.zig", "_dummy_plugin.zig"] do
        File.cp_r!(Path.join(source, file), Path.join(build_source, file))
      end

      launcher = Path.join(build_source, "src/erlang_launcher.zig")
      original = File.read!(launcher)
      anchor = "        \"-extra\",\n"

      unless length(String.split(original, anchor)) == 2 do
        raise "Burrito launcher changed: review the server --no-halt adaptation before releasing"
      end

      File.write!(
        launcher,
        String.replace(original, anchor, anchor <> "        \"--no-halt\",\n")
      )

      burrito_options =
        release.options[:burrito]
        |> Keyword.put(:server_source, build_source)
        |> Keyword.put(:extra_steps, fetch: [pre: [__MODULE__]])

      Burrito.wrap(%{release | options: Keyword.put(release.options, :burrito, burrito_options)})
      release
    after
      File.rm_rf!(build_source)
    end
  end

  # Burrito's documented extra-step hook lets every fetch/patch/build phase
  # use the isolated compiler source, including the generated musl runtime.
  def execute(context) do
    %{context | self_dir: context.mix_release.options[:burrito][:server_source]}
  end
end
