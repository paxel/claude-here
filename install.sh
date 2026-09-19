#!/usr/bin/env sh
# Install the latest claude_here release binaries into ~/.local/bin (or $CLAUDE_HERE_BIN_DIR).
#   curl -fsSL https://github.com/paxel/claude-here/releases/latest/download/install.sh | sh
set -eu

repo="paxel/claude-here"
bin_dir="${CLAUDE_HERE_BIN_DIR:-$HOME/.local/bin}"
version="${CLAUDE_HERE_VERSION:-}"

os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
  Linux) os_part="unknown-linux-musl" ;;
  Darwin) os_part="apple-darwin" ;;
  *) echo "unsupported OS: $os" >&2; exit 1 ;;
esac
case "$arch" in
  x86_64|amd64) arch_part="x86_64" ;;
  aarch64|arm64) arch_part="aarch64" ;;
  *) echo "unsupported architecture: $arch" >&2; exit 1 ;;
esac
target="${arch_part}-${os_part}"

if [ -z "$version" ]; then
  version="$(curl -fsSL "https://api.github.com/repos/${repo}/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n1)"
fi
[ -n "$version" ] || { echo "could not determine latest version" >&2; exit 1; }

name="claude_here-${version}-${target}"
url="https://github.com/${repo}/releases/download/${version}/${name}.tar.gz"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "downloading ${url}"
curl -fsSL -o "$tmp/$name.tar.gz" "$url"
curl -fsSL -o "$tmp/$name.tar.gz.sha256" "$url.sha256"
(cd "$tmp" && (shasum -a 256 -c "$name.tar.gz.sha256" >/dev/null 2>&1 || sha256sum -c "$name.tar.gz.sha256" >/dev/null))
tar -xzf "$tmp/$name.tar.gz" -C "$tmp"
mkdir -p "$bin_dir"
install -m 755 "$tmp/$name/claude_here" "$tmp/$name/claude_yolo" "$bin_dir/"
echo "installed claude_here ${version} to ${bin_dir}"
case ":$PATH:" in
  *":$bin_dir:"*) ;;
  *) echo "note: ${bin_dir} is not on your PATH" ;;
esac
echo "next: claude_here init"
