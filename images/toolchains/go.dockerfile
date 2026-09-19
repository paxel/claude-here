# go toolchain: Go plus the gopls language server.
ARG BASE=claude_here:base
FROM ${BASE}
ARG GO_VERSION=1.25.1

RUN arch="$(dpkg --print-architecture)"; \
    case "$arch" in amd64) ga=amd64 ;; arm64) ga=arm64 ;; *) echo "unsupported arch $arch"; exit 1 ;; esac; \
    curl -fsSL "https://go.dev/dl/go${GO_VERSION}.linux-${ga}.tar.gz" | tar -xz -C /opt \
 && ln -s /opt/go/bin/go /usr/local/bin/go \
 && ln -s /opt/go/bin/gofmt /usr/local/bin/gofmt
ENV GOROOT=/opt/go
RUN GOBIN=/usr/local/bin GOFLAGS=-mod=mod go install golang.org/x/tools/gopls@latest \
 && go version && gopls version
