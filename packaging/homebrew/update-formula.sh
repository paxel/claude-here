#!/usr/bin/env sh
# Render claude-here.rb for a released version by fetching the sha256 files.
# Usage: packaging/homebrew/update-formula.sh 0.1.0 > /path/to/homebrew-tap/Formula/claude-here.rb
set -eu
v="$1"
base="https://github.com/paxel/claude-here/releases/download/v${v}"
sha() { curl -fsSL "${base}/claude_here-v${v}-$1.tar.gz.sha256" | awk '{print $1}'; }
sed \
  -e "s/VERSION/${v}/g" \
  -e "s/SHA_MAC_ARM/$(sha aarch64-apple-darwin)/" \
  -e "s/SHA_MAC_X86/$(sha x86_64-apple-darwin)/" \
  -e "s/SHA_LINUX_ARM/$(sha aarch64-unknown-linux-musl)/" \
  -e "s/SHA_LINUX_X86/$(sha x86_64-unknown-linux-musl)/" \
  "$(dirname "$0")/claude-here.rb" | grep -v '^# '
