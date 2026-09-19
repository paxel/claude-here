# syntax=docker/dockerfile:1.7
# azure toolchain: the Azure CLI, installed into its own venv so it stays out of
# the way of any project python.
ARG BASE=claude_here:base
FROM ${BASE}

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    apt-get update \
 && apt-get install -y --no-install-recommends python3-dev libffi-dev
RUN --mount=type=cache,target=/root/.cache/pip,sharing=locked \
    python3 -m venv /opt/azure-cli \
 && /opt/azure-cli/bin/pip install --upgrade pip \
 && /opt/azure-cli/bin/pip install azure-cli \
 && ln -s /opt/azure-cli/bin/az /usr/local/bin/az \
 && az version --output tsv | head -1
