# dart toolchain: the Flutter SDK, which carries the Dart SDK and the Dart
# language server. Installed into /opt, cloned *as the sandbox user* so no
# `chown -R` has to copy a gigabyte of checkout into a second layer.
ARG BASE=claude_here:base
FROM ${BASE}
ARG CH_USER=ni
ARG FLUTTER_CHANNEL=stable

# System scope, not the user's global git config: the container home is a mount
# at run time and would shadow anything written to ~/.gitconfig here.
RUN mkdir -p /opt/flutter \
 && chown "${CH_USER}" /opt/flutter \
 && git config --system --add safe.directory /opt/flutter

ENV FLUTTER_ROOT=/opt/flutter \
    PUB_CACHE=/home/${CH_USER}/.pub-cache \
    PATH="/opt/flutter/bin:/opt/flutter/bin/cache/dart-sdk/bin:${PATH}"

USER ${CH_USER}
# The directory itself must survive a retry: /opt is root-owned, so the user
# could not recreate it.
RUN for attempt in 1 2 3; do \
      find /opt/flutter -mindepth 1 -delete; \
      git clone --depth 1 --branch "${FLUTTER_CHANNEL}" \
        https://github.com/flutter/flutter.git /opt/flutter && break; \
      echo "flutter clone attempt ${attempt} failed; retrying"; sleep 5; \
    done \
 && test -d /opt/flutter/bin

RUN flutter config --no-analytics >/dev/null \
 && dart --disable-analytics >/dev/null \
 && flutter precache --universal \
 && flutter --version && dart --version
USER root
