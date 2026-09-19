# gcloud toolchain: the Google Cloud CLI. Large (~1GB unpacked); enabled per
# project, never by default.
ARG BASE=claude_here:base
FROM ${BASE}
ARG GCLOUD_VERSION=512.0.0

RUN arch="$(dpkg --print-architecture)"; \
    case "$arch" in amd64) a=x86_64 ;; arm64) a=arm ;; *) echo "unsupported arch $arch"; exit 1 ;; esac; \
    curl -fsSL --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20 -o /tmp/gcloud.tar.gz \
      "https://dl.google.com/dl/cloudsdk/channels/rapid/downloads/google-cloud-cli-${GCLOUD_VERSION}-linux-${a}.tar.gz" \
 && tar -xzf /tmp/gcloud.tar.gz -C /opt && rm /tmp/gcloud.tar.gz \
 && /opt/google-cloud-sdk/install.sh --quiet --usage-reporting false --path-update false --command-completion false \
 && ln -s /opt/google-cloud-sdk/bin/gcloud /usr/local/bin/gcloud \
 && ln -s /opt/google-cloud-sdk/bin/gsutil /usr/local/bin/gsutil \
 && gcloud --version | head -1
