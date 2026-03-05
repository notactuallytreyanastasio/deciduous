class DeciduousAT014 < Formula
  desc "Decision graph tooling for AI-assisted development (beta)"
  homepage "https://notactuallytreyanastasio.github.io/deciduous/"
  version "0.14.0-beta.2"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/notactuallytreyanastasio/deciduous/releases/download/v#{version}/deciduex_darwin_arm64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_DARWIN_ARM64" # darwin_arm64
    end
    on_intel do
      url "https://github.com/notactuallytreyanastasio/deciduous/releases/download/v#{version}/deciduex_darwin_amd64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_DARWIN_AMD64" # darwin_amd64
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/notactuallytreyanastasio/deciduous/releases/download/v#{version}/deciduex_linux_amd64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_AMD64" # linux_amd64
    end
  end

  def install
    # Burrito produces a single executable
    burrito_binary = Dir["deciduex_*"].first
    if burrito_binary
      bin.install burrito_binary => "deciduous"
    else
      odie "No deciduex binary found in tarball"
    end
  end

  def caveats
    <<~EOS
      This is a BETA release of deciduous (Elixir rewrite).

      To use the stable Rust version instead:
        brew uninstall deciduous@0.14
        brew install deciduous

      Report issues at:
        https://github.com/notactuallytreyanastasio/deciduous/issues
    EOS
  end

  test do
    assert_match "deciduous", shell_output("#{bin}/deciduous --help 2>&1", 1)
  end
end
