# syntax=docker/dockerfile:1.7
# go toolchain: Go plus the gopls language server.
ARG BASE=claude_here:base
FROM ${BASE}
ARG GO_VERSION=1.27.1
# `latest` is the default so a fresh install is current; pin it for a
# reproducible layer.
ARG GOPLS_VERSION=latest

RUN arch="$(dpkg --print-architecture)"; \
    case "$arch" in amd64) ga=amd64 ;; arm64) ga=arm64 ;; *) echo "unsupported arch $arch"; exit 1 ;; esac; \
    curl -fsSL --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20 -o /tmp/go.tar.gz "https://go.dev/dl/go${GO_VERSION}.linux-${ga}.tar.gz" \
 && tar -xzf /tmp/go.tar.gz -C /opt && rm /tmp/go.tar.gz \
 && ln -s /opt/go/bin/go /usr/local/bin/go \
 && ln -s /opt/go/bin/gofmt /usr/local/bin/gofmt
ENV GOROOT=/opt/go
RUN --mount=type=cache,target=/root/.cache/go-build,sharing=locked \
    --mount=type=cache,target=/root/go/pkg/mod,sharing=locked \
    GOBIN=/usr/local/bin GOFLAGS=-mod=mod \
    go install "golang.org/x/tools/gopls@${GOPLS_VERSION}" \
 && go version && gopls version
