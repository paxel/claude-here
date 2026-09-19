# python toolchain: poetry, ruff, mypy, uv and pyright. python3, pip and venv
# are already in the base image. Implies `node`, because pyright is a node
# program; the tools themselves live in their own venv so they never fight
# with a project environment or a PEP 668 managed system python.
ARG BASE=claude_here:base
FROM ${BASE}
RUN python3 -m venv /opt/pytools \
 && /opt/pytools/bin/pip install --no-cache-dir --upgrade pip \
 && /opt/pytools/bin/pip install --no-cache-dir poetry ruff mypy \
 && for b in poetry ruff mypy; do ln -s "/opt/pytools/bin/$b" "/usr/local/bin/$b"; done \
 && poetry --version && ruff --version && mypy --version
RUN npm install -g pyright && pyright-langserver --version
