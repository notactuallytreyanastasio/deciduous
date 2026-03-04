class Deciduous < Formula
  desc "Decision graph tooling for AI-assisted development"
  homepage "https://notactuallytreyanastasio.github.io/deciduous/"
  version "0.13.2"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/notactuallytreyanastasio/deciduous/releases/download/v#{version}/deciduous-darwin-arm64"
      sha256 "77e6840f5f1db908332b5d77a4bdf14a75dffc1b79c48b1857e3df17157c4bf7"

      resource "deciduex" do
        url "https://github.com/notactuallytreyanastasio/deciduous/releases/download/v#{version}/deciduex-darwin-arm64.tar.gz"
        sha256 "0000000000000000000000000000000000000000000000000000000000000000"
      end
    end
    on_intel do
      url "https://github.com/notactuallytreyanastasio/deciduous/releases/download/v#{version}/deciduous-darwin-amd64"
      sha256 "75f43c25852d67e15b94e26f64df7132e55bd92989aded4f76216d236d962362"

      resource "deciduex" do
        url "https://github.com/notactuallytreyanastasio/deciduous/releases/download/v#{version}/deciduex-darwin-amd64.tar.gz"
        sha256 "0000000000000000000000000000000000000000000000000000000000000000"
      end
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/notactuallytreyanastasio/deciduous/releases/download/v#{version}/deciduous-linux-arm64"
      sha256 "209fde4d4d501258cccacfa2166e14a165873399412f99ab011b909187491614"

      resource "deciduex" do
        url "https://github.com/notactuallytreyanastasio/deciduous/releases/download/v#{version}/deciduex-linux-arm64.tar.gz"
        sha256 "0000000000000000000000000000000000000000000000000000000000000000"
      end
    end
    on_intel do
      url "https://github.com/notactuallytreyanastasio/deciduous/releases/download/v#{version}/deciduous-linux-amd64"
      sha256 "8e79950375ed2d4e0cc2a031da8080284ee7989cb7738f8b05ac817a9c486514"

      resource "deciduex" do
        url "https://github.com/notactuallytreyanastasio/deciduous/releases/download/v#{version}/deciduex-linux-amd64.tar.gz"
        sha256 "0000000000000000000000000000000000000000000000000000000000000000"
      end
    end
  end

  def install
    # Install Rust binary
    binary_name = Dir["deciduous-*"].first
    bin.install binary_name => "deciduous"

    # Install Elixir release under libexec
    resource("deciduex").stage do
      (libexec/"deciduex").install Dir["deciduex/*"]
    end
    chmod 0755, libexec/"deciduex/bin/cli"
    chmod 0755, libexec/"deciduex/bin/deciduex"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/deciduous --version")
  end
end
