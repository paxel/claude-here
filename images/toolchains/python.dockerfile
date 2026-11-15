# syntax=docker/dockerfile:1.7
# python toolchain: poetry, ruff, mypy, uv and pyright. python3, pip and venv
# are already in the base image. Implies `node`, because pyright is a node
# program; the tools themselves live in their own venv so they never fight
# with a project environment or a PEP 668 managed system python.
ARG BASE=claude_here:base
FROM ${BASE}
# Every package and its hash come from requirements.txt and package-lock.json.
COPY requirements.txt /opt/pytools-requirements.txt
RUN --mount=type=cache,target=/root/.cache/pip,sharing=locked \
    python3 -m venv /opt/pytools \
 && /opt/pytools/bin/pip install --require-hashes --no-deps -r /opt/pytools-requirements.txt \
 && for b in poetry ruff mypy; do ln -s "/opt/pytools/bin/$b" "/usr/local/bin/$b"; done \
 && poetry --version && ruff --version && mypy --version
COPY pyright-package.json /opt/pyright/package.json
COPY pyright-package-lock.json /opt/pyright/package-lock.json
RUN --mount=type=cache,target=/root/.npm,sharing=locked \
    npm ci --prefix /opt/pyright --ignore-scripts --no-audit --no-fund \
 && ln -s /opt/pyright/node_modules/.bin/pyright /opt/pyright/node_modules/.bin/pyright-langserver /usr/local/bin/ \
 && pyright --version && command -v pyright-langserver
