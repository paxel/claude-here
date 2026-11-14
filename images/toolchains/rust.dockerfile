# rust toolchain: rustup stable with clippy, rustfmt and rust-analyzer.
ARG BASE=claude_here:base
FROM ${BASE}
ARG CH_USER=ni
# rustup-init is checked against the sha256 that static.rust-lang.org
# publishes next to it before it runs.
# renovate: datasource=github-tags depName=rust-lang/rustup
ARG RUSTUP_VERSION=1.29.3
USER ${CH_USER}
ENV RUSTUP_HOME=/home/${CH_USER}/.rustup \
    CARGO_HOME=/home/${CH_USER}/.cargo
RUN set -eu \
 && case "$(uname -m)" in \
      x86_64) t=x86_64-unknown-linux-gnu ;; \
      aarch64) t=aarch64-unknown-linux-gnu ;; \
      *) echo "rust: unsupported architecture $(uname -m)" >&2; exit 1 ;; \
    esac \
 && url="https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/${t}/rustup-init" \
 && curl -fsSL --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20 -o /tmp/rustup-init "${url}" \
 && sum="$(curl -fsSL --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20 "${url}.sha256" | cut -d' ' -f1)" \
 && echo "${sum}  /tmp/rustup-init" | sha256sum -c - \
 && chmod +x /tmp/rustup-init \
 && /tmp/rustup-init -y --profile minimal --component clippy,rustfmt,rust-analyzer \
 && rm /tmp/rustup-init \
 && /home/${CH_USER}/.cargo/bin/cargo --version \
 && /home/${CH_USER}/.cargo/bin/rust-analyzer --version
USER root
ENV PATH="/home/${CH_USER}/.cargo/bin:${PATH}"
