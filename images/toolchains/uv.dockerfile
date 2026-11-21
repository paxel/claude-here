# uv toolchain: uv/uvx, the Rust-based Python package manager. Kept separate
# from `python` because `uvx` is how many MCP servers are launched.
ARG BASE=claude_here:base
FROM ${BASE}
# The release archive is checked against the sha256 published with it.
ARG UV_VERSION=0.13.8
RUN set -eu \
 && case "$(uname -m)" in \
      x86_64) t=x86_64-unknown-linux-gnu ;; \
      aarch64) t=aarch64-unknown-linux-gnu ;; \
      *) echo "uv: unsupported architecture $(uname -m)" >&2; exit 1 ;; \
    esac \
 && url="https://github.com/astral-sh/uv/releases/download/${UV_VERSION}/uv-${t}.tar.gz" \
 && curl -fsSL --proto =https --tlsv1.2 --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20 -o /tmp/uv.tar.gz "${url}" \
 && sum="$(curl -fsSL --proto =https --tlsv1.2 --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20 "${url}.sha256" | cut -d' ' -f1)" \
 && echo "${sum}  /tmp/uv.tar.gz" | sha256sum -c - \
 && tar -xzf /tmp/uv.tar.gz -C /tmp \
 && install -m 0755 "/tmp/uv-${t}/uv" "/tmp/uv-${t}/uvx" /usr/local/bin/ \
 && rm -rf /tmp/uv.tar.gz "/tmp/uv-${t}" \
 && uv --version && uvx --version
