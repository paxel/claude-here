# syntax=docker/dockerfile:1.7
# node toolchain: Node.js with its npm, pnpm, yarn, TypeScript and the
# TypeScript language server. Many MCP servers and plugin hooks need node.
ARG BASE=claude_here:base
FROM ${BASE}
# 22 is LTS ("Jod") with support into 2027.
ARG NODE_MAJOR=22
# NodeSource's apt signing key, pinned by its sha256; apt then checks every
# package of the repository against it.
ARG NODESOURCE_KEY_SHA256=b42e0321dabdc24e892115da705cf061167eac12a317f23d329862d0aa0a271d

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    curl -fsSL --proto =https --tlsv1.2 --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20 \
      -o /tmp/nodesource.key https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key \
 && echo "${NODESOURCE_KEY_SHA256}  /tmp/nodesource.key" | sha256sum -c - \
 && gpg --dearmor < /tmp/nodesource.key > /usr/share/keyrings/nodesource.gpg \
 && rm /tmp/nodesource.key \
 && echo "deb [signed-by=/usr/share/keyrings/nodesource.gpg] https://deb.nodesource.com/node_${NODE_MAJOR}.x nodistro main" \
      > /etc/apt/sources.list.d/nodesource.list \
 && printf 'Package: nodejs\nPin: origin deb.nodesource.com\nPin-Priority: 600\n' > /etc/apt/preferences.d/nodesource \
 && apt-get update \
 && apt-get install -y --no-install-recommends nodejs
# The global tools come from package-lock.json: `npm ci` installs exactly the
# locked versions and checks every package's integrity hash.
COPY package.json package-lock.json /opt/node-tools/
RUN --mount=type=cache,target=/root/.npm,sharing=locked \
    npm ci --prefix /opt/node-tools --ignore-scripts --no-audit --no-fund \
 && ln -s /opt/node-tools/node_modules/.bin/* /usr/local/bin/ \
 && node --version && npm --version && npx --version \
 && pnpm --version && yarn --version && tsc --version \
 && command -v typescript-language-server
