# Homebrew formula for claude_here. Copy into paxel/homebrew-tap as
# Formula/claude-here.rb after a release; fill the sha256 values with
# packaging/homebrew/update-formula.sh <version>.
class ClaudeHere < Formula
  desc "Run Claude Code in a project-scoped Docker sandbox with enforced git modes"
  homepage "https://github.com/paxel/claude-here"
  version "VERSION"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/paxel/claude-here/releases/download/vVERSION/claude_here-vVERSION-aarch64-apple-darwin.tar.gz"
      sha256 "SHA_MAC_ARM"
    end
    on_intel do
      url "https://github.com/paxel/claude-here/releases/download/vVERSION/claude_here-vVERSION-x86_64-apple-darwin.tar.gz"
      sha256 "SHA_MAC_X86"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/paxel/claude-here/releases/download/vVERSION/claude_here-vVERSION-aarch64-unknown-linux-musl.tar.gz"
      sha256 "SHA_LINUX_ARM"
    end
    on_intel do
      url "https://github.com/paxel/claude-here/releases/download/vVERSION/claude_here-vVERSION-x86_64-unknown-linux-musl.tar.gz"
      sha256 "SHA_LINUX_X86"
    end
  end

  def install
    bin.install "claude_here", "claude_yolo"
    generate_completions_from_executable(bin/"claude_here", "completions")
  end

  def caveats
    <<~EOS
      Requires Docker. Run `claude_here init` once to obtain a Claude token
      and seed the container home.
    EOS
  end

  test do
    assert_match "claude_here", shell_output("#{bin}/claude_here --version")
  end
end
