class Rpotato < Formula
  desc "Local coding agents for potato PCs."
  homepage "https://github.com/MCprotein/rolling-potato"
  version "0.40.0"
  license "Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/MCprotein/rolling-potato/releases/download/v0.40.0/rpotato-v0.40.0-aarch64-apple-darwin.tar.gz"
      sha256 "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    else
      url "https://github.com/MCprotein/rolling-potato/releases/download/v0.40.0/rpotato-v0.40.0-x86_64-apple-darwin.tar.gz"
      sha256 "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/MCprotein/rolling-potato/releases/download/v0.40.0/rpotato-v0.40.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
    else
      url "https://github.com/MCprotein/rolling-potato/releases/download/v0.40.0/rpotato-v0.40.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
    end
  end

  def install
    bin.install "rpotato"
  end

  test do
    assert_match "package version: 0.40.0", shell_output("#{bin}/rpotato doctor")
  end
end
