# android toolchain: the Android command line tools and platform-tools, baked
# into one SDK root owned by the sandbox user — so no `chown -R` duplicates the
# SDK into a second layer. Platforms, build-tools and NDK are *not* baked: they
# are mounted from the host SDK (see the `android` cache), so no API level has to
# be guessed here and Gradle can install what a project asks for. Implies `jvm`.
# The emulator is deliberately absent: it needs /dev/kvm and a GUI, and the
# sandbox drops all capabilities.
ARG BASE=claude_here:base
FROM ${BASE}
ARG CH_USER=ni
ARG CMDLINE_TOOLS_VERSION=11076708
ENV ANDROID_HOME=/opt/android-sdk \
    ANDROID_SDK_ROOT=/opt/android-sdk
ENV PATH="${ANDROID_HOME}/cmdline-tools/latest/bin:${ANDROID_HOME}/platform-tools:${PATH}"

RUN mkdir -p "${ANDROID_HOME}" && chown "${CH_USER}" "${ANDROID_HOME}"
USER ${CH_USER}

RUN mkdir -p "${ANDROID_HOME}/cmdline-tools" \
 && curl -fsSL --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20 -o /tmp/tools.zip \
      "https://dl.google.com/android/repository/commandlinetools-linux-${CMDLINE_TOOLS_VERSION}_latest.zip" \
 && unzip -q /tmp/tools.zip -d "${ANDROID_HOME}/cmdline-tools" \
 && mv "${ANDROID_HOME}/cmdline-tools/cmdline-tools" "${ANDROID_HOME}/cmdline-tools/latest" \
 && rm /tmp/tools.zip

# Licenses are accepted here, not at run time: the mounted package directories
# are empty on a fresh host and Gradle must be able to fetch into them.
RUN yes | sdkmanager --licenses >/dev/null \
 && sdkmanager --install "platform-tools" >/dev/null \
 && mkdir -p "${ANDROID_HOME}/platforms" "${ANDROID_HOME}/build-tools" "${ANDROID_HOME}/ndk" \
 && adb --version
USER root
