# rust toolchain: rustup stable with clippy, rustfmt and rust-analyzer.
ARG BASE=claude_here:base
FROM ${BASE}
ARG CH_USER=ni
USER ${CH_USER}
ENV RUSTUP_HOME=/home/${CH_USER}/.rustup \
    CARGO_HOME=/home/${CH_USER}/.cargo
RUN curl -fsSL https://sh.rustup.rs \
      | sh -s -- -y --profile minimal --component clippy,rustfmt,rust-analyzer \
 && /home/${CH_USER}/.cargo/bin/cargo --version \
 && /home/${CH_USER}/.cargo/bin/rust-analyzer --version
USER root
ENV PATH="/home/${CH_USER}/.cargo/bin:${PATH}"
