# aws toolchain: AWS CLI v2.
ARG BASE=claude_here:base
FROM ${BASE}

RUN arch="$(dpkg --print-architecture)"; \
    case "$arch" in amd64) a=x86_64 ;; arm64) a=aarch64 ;; *) echo "unsupported arch $arch"; exit 1 ;; esac; \
    curl -fsSL -o /tmp/aws.zip "https://awscli.amazonaws.com/awscli-exe-linux-${a}.zip" \
 && unzip -q /tmp/aws.zip -d /tmp \
 && /tmp/aws/install --bin-dir /usr/local/bin --install-dir /opt/aws-cli \
 && rm -rf /tmp/aws /tmp/aws.zip \
 && aws --version
