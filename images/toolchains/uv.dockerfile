# uv toolchain: uv/uvx, the Rust-based Python package manager. Kept separate
# from `python` because `uvx` is how many MCP servers are launched.
ARG BASE=claude_here:base
FROM ${BASE}
RUN curl -LsSf --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20 https://astral.sh/uv/install.sh \
      | UV_INSTALL_DIR=/usr/local/bin UV_NO_MODIFY_PATH=1 sh \
 && uv --version && uvx --version
