# claude_here Claude Code layer: the native binary, installed into the sandbox
# user's home. It is the last layer of every chain, above the toolchains and the
# user and project layers, so a new Claude release rebuilds this layer alone.
# CLAUDE_VERSION is an exact version resolved on the host (or a pin), and it is
# part of the layer hash.
ARG BASE=claude_here:base
FROM ${BASE}
ARG CH_USER=ni
ARG CLAUDE_VERSION=latest

USER ${CH_USER}
RUN curl -fsSL --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20 https://claude.ai/install.sh | bash -s -- "${CLAUDE_VERSION}" \
 && "/home/${CH_USER}/.local/bin/claude" --version
USER root
