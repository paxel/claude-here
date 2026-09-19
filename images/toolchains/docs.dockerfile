# docs toolchain: diagram and document rendering, so a diagram can be written,
# rendered and then looked at with the Read tool instead of written blind.
# graphviz lives in the base image. A headless JRE is installed only when the
# image has no java yet, so `--jvm --docs` reuses GraalVM.
ARG BASE=claude_here:base
FROM ${BASE}
ARG PLANTUML_VERSION=1.2026.8
ARG D2_VERSION=0.7.0
ARG TYPST_VERSION=0.15.1
ARG PANDOC_VERSION=3.11

RUN if ! command -v java >/dev/null 2>&1; then \
      apt-get update \
   && apt-get install -y --no-install-recommends default-jre-headless \
   && rm -rf /var/lib/apt/lists/*; \
    fi

RUN arch="$(dpkg --print-architecture)"; \
    case "$arch" in \
      amd64) d2=amd64; ty=x86_64; pd=amd64 ;; \
      arm64) d2=arm64; ty=aarch64; pd=arm64 ;; \
      *) echo "unsupported arch $arch"; exit 1 ;; \
    esac; \
    mkdir -p /opt/plantuml \
 && curl -fsSL --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20 -o /opt/plantuml/plantuml.jar \
      "https://github.com/plantuml/plantuml/releases/download/v${PLANTUML_VERSION}/plantuml-${PLANTUML_VERSION}.jar" \
 && printf '#!/bin/sh\nexec java -jar /opt/plantuml/plantuml.jar "$@"\n' > /usr/local/bin/plantuml \
 && chmod 755 /usr/local/bin/plantuml \
 && curl -fsSL --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20 -o /tmp/d2.tar.gz \
      "https://github.com/terrastruct/d2/releases/download/v${D2_VERSION}/d2-v${D2_VERSION}-linux-${d2}.tar.gz" \
 && tar -xzf /tmp/d2.tar.gz -C /tmp && rm /tmp/d2.tar.gz \
 && install -m 755 "/tmp/d2-v${D2_VERSION}/bin/d2" /usr/local/bin/d2 \
 && rm -rf "/tmp/d2-v${D2_VERSION}" \
 && curl -fsSL --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20 -o /tmp/typst.tar.xz \
      "https://github.com/typst/typst/releases/download/v${TYPST_VERSION}/typst-${ty}-unknown-linux-musl.tar.xz" \
 && tar -xJf /tmp/typst.tar.xz -C /tmp && rm /tmp/typst.tar.xz \
 && install -m 755 "/tmp/typst-${ty}-unknown-linux-musl/typst" /usr/local/bin/typst \
 && rm -rf "/tmp/typst-${ty}-unknown-linux-musl" \
 && curl -fsSL --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20 -o /tmp/pandoc.tar.gz \
      "https://github.com/jgm/pandoc/releases/download/${PANDOC_VERSION}/pandoc-${PANDOC_VERSION}-linux-${pd}.tar.gz" \
 && tar -xzf /tmp/pandoc.tar.gz -C /tmp && rm /tmp/pandoc.tar.gz \
 && install -m 755 "/tmp/pandoc-${PANDOC_VERSION}/bin/pandoc" /usr/local/bin/pandoc \
 && rm -rf "/tmp/pandoc-${PANDOC_VERSION}" \
 && plantuml -version && d2 --version && typst --version && pandoc --version | head -1
