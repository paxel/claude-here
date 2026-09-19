# k8s toolchain: kubectl, helm and kustomize. Without mounted credentials these
# render and validate manifests; reaching a live cluster needs a cloud mode
# other than `none` (ADR 0005).
ARG BASE=claude_here:base
FROM ${BASE}
ARG KUBECTL_VERSION=v1.37.0
# Helm 4 changes CLI behaviour; staying on 3.x until that is a deliberate move.
ARG HELM_VERSION=v3.16.4
ARG KUSTOMIZE_VERSION=v5.8.1

RUN arch="$(dpkg --print-architecture)"; \
    case "$arch" in amd64) a=amd64 ;; arm64) a=arm64 ;; *) echo "unsupported arch $arch"; exit 1 ;; esac; \
    curl -fsSL --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20 -o /usr/local/bin/kubectl \
      "https://dl.k8s.io/release/${KUBECTL_VERSION}/bin/linux/${a}/kubectl" \
 && chmod 755 /usr/local/bin/kubectl \
 && curl -fsSL --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20 -o /tmp/helm.tar.gz "https://get.helm.sh/helm-${HELM_VERSION}-linux-${a}.tar.gz" \
 && tar -xzf /tmp/helm.tar.gz -C /tmp && rm /tmp/helm.tar.gz \
 && install -m 755 "/tmp/linux-${a}/helm" /usr/local/bin/helm \
 && rm -rf "/tmp/linux-${a}" \
 && curl -fsSL --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20 -o /tmp/kustomize.tar.gz \
      "https://github.com/kubernetes-sigs/kustomize/releases/download/kustomize%2F${KUSTOMIZE_VERSION}/kustomize_${KUSTOMIZE_VERSION}_linux_${a}.tar.gz" \
 && tar -xzf /tmp/kustomize.tar.gz -C /usr/local/bin && rm /tmp/kustomize.tar.gz \
 && chmod 755 /usr/local/bin/kustomize \
 && kubectl version --client=true --output=yaml | head -3 \
 && helm version --short && kustomize version
