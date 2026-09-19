# terraform toolchain. `plan` is allowed in cloud mode `ro`, although it can
# take a state lock on a remote backend (ADR 0005).
ARG BASE=claude_here:base
FROM ${BASE}
ARG TERRAFORM_VERSION=1.10.5

RUN arch="$(dpkg --print-architecture)"; \
    case "$arch" in amd64) a=amd64 ;; arm64) a=arm64 ;; *) echo "unsupported arch $arch"; exit 1 ;; esac; \
    curl -fsSL -o /tmp/tf.zip \
      "https://releases.hashicorp.com/terraform/${TERRAFORM_VERSION}/terraform_${TERRAFORM_VERSION}_linux_${a}.zip" \
 && unzip -q /tmp/tf.zip -d /usr/local/bin && rm /tmp/tf.zip \
 && chmod 755 /usr/local/bin/terraform \
 && terraform version
