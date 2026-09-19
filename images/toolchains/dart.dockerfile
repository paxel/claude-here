# dart toolchain: the Flutter SDK, which carries the Dart SDK and the Dart
# language server. Installed into /opt so the container home stays free, owned
# by the sandbox user because flutter writes into its own directory.
ARG BASE=claude_here:base
FROM ${BASE}
ARG CH_USER=ni
ARG FLUTTER_CHANNEL=stable

RUN for attempt in 1 2 3; do \
      rm -rf /opt/flutter; \
      git clone --depth 1 --branch "${FLUTTER_CHANNEL}" \
        https://github.com/flutter/flutter.git /opt/flutter && break; \
      echo "flutter clone attempt ${attempt} failed; retrying"; sleep 5; \
    done \
 && test -d /opt/flutter/bin \
 && chown -R "${CH_USER}" /opt/flutter
ENV FLUTTER_ROOT=/opt/flutter \
    PUB_CACHE=/home/${CH_USER}/.pub-cache \
    PATH="/opt/flutter/bin:/opt/flutter/bin/cache/dart-sdk/bin:${PATH}"
USER ${CH_USER}
RUN git config --global --add safe.directory /opt/flutter \
 && flutter config --no-analytics >/dev/null \
 && dart --disable-analytics >/dev/null \
 && flutter precache --universal \
 && flutter --version && dart --version
USER root
