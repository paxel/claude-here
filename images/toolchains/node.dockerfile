# syntax=docker/dockerfile:1.7
# node toolchain: Node.js with the latest npm, pnpm, yarn, TypeScript and the
# TypeScript language server. Many MCP servers and plugin hooks need node.
ARG BASE=claude_here:base
FROM ${BASE}
# 22 is LTS ("Jod") with support into 2027.
ARG NODE_MAJOR=22

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    curl -fsSL --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20 "https://deb.nodesource.com/setup_${NODE_MAJOR}.x" | bash - \
 && apt-get install -y --no-install-recommends nodejs
RUN --mount=type=cache,target=/root/.npm,sharing=locked \
    npm install -g npm@latest pnpm yarn typescript typescript-language-server \
 && node --version && npm --version && npx --version \
 && pnpm --version && yarn --version && tsc --version
