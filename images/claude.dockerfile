# claude_here Claude Code layer: the native binary, installed into the sandbox
# user's home. It is the last layer of every chain, above the toolchains and the
# user and project layers, so a new Claude release rebuilds this layer alone.
# CLAUDE_VERSION is an exact version resolved on the host (or a pin), and it is
# part of the layer hash.
#
# The binary is downloaded the way Claude's own install.sh does it, without
# piping that script into a shell: the release manifest names the sha256 of
# each platform's binary, and the binary must match it before it runs.
ARG BASE=claude_here:base
FROM ${BASE}
ARG CH_USER=ni
ARG CLAUDE_VERSION=latest

USER ${CH_USER}
RUN set -eu \
 && base=https://downloads.claude.ai/claude-code-releases \
 && get() { curl -fsSL --proto =https --tlsv1.2 --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20 "$@"; } \
 && case "$(uname -m)" in \
      x86_64) platform=linux-x64 ;; \
      aarch64) platform=linux-arm64 ;; \
      *) echo "claude: unsupported architecture $(uname -m)" >&2; exit 1 ;; \
    esac \
 && version="${CLAUDE_VERSION}" \
 && case "${version}" in latest|stable) version="$(get "${base}/${version}")" ;; esac \
 && sum="$(get "${base}/${version}/manifest.json" | jq -r --arg p "${platform}" '.platforms[$p].checksum')" \
 && echo "${sum}" | grep -Eq '^[0-9a-f]{64}$' \
 && get -o /tmp/claude-download "${base}/${version}/${platform}/claude" \
 && echo "${sum}  /tmp/claude-download" | sha256sum -c - \
 && chmod +x /tmp/claude-download \
 && /tmp/claude-download install "${version}" \
 && rm -f /tmp/claude-download \
 && "/home/${CH_USER}/.local/bin/claude" --version
USER root
