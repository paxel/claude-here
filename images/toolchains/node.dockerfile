# node toolchain: Node.js with the latest npm, pnpm, yarn, TypeScript and the
# TypeScript language server. Many MCP servers and plugin hooks need node.
ARG BASE=claude_here:base
FROM ${BASE}
ARG NODE_MAJOR=22

RUN curl -fsSL "https://deb.nodesource.com/setup_${NODE_MAJOR}.x" | bash - \
 && apt-get install -y --no-install-recommends nodejs \
 && rm -rf /var/lib/apt/lists/* \
 && npm install -g npm@latest pnpm yarn typescript typescript-language-server \
 && node --version && npm --version && npx --version \
 && pnpm --version && yarn --version && tsc --version
