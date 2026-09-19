# jvm toolchain: GraalVM CE 21 (JDK + native-image), Maven, Gradle, Kotlin,
# and the Eclipse JDT language server.
#
# Every download is retried and written to a file before it is unpacked: a
# retried pipe would hand tar a restarted stream, and one DNS blip must not
# throw away the rest of a long build.
ARG BASE=claude_here:base
FROM ${BASE}
# GraalVM CE stopped publishing jdk-21.0.x tags after 21.0.2 (newer CE builds
# use graal-25.x naming); this is the last one at this URL pattern.
ARG GRAALVM_VERSION=21.0.2
ARG MAVEN_VERSION=3.9.16
# Gradle 9 is a major step with removed deprecations. Projects use their own
# wrapper anyway, so this stays on 8.x until someone needs otherwise.
ARG GRADLE_VERSION=8.14
ARG KOTLIN_VERSION=2.4.20
# ARG, not ENV: build scaffolding has no business in the runtime environment.
ARG CH_CURL="curl -fsSL --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20"

RUN arch="$(dpkg --print-architecture)"; \
    case "$arch" in amd64) ga=x64 ;; arm64) ga=aarch64 ;; *) echo "unsupported arch $arch"; exit 1 ;; esac; \
    mkdir -p /opt/graalvm \
 && ${CH_CURL} -o /tmp/graalvm.tar.gz \
      "https://github.com/graalvm/graalvm-ce-builds/releases/download/jdk-${GRAALVM_VERSION}/graalvm-community-jdk-${GRAALVM_VERSION}_linux-${ga}_bin.tar.gz" \
 && tar -xzf /tmp/graalvm.tar.gz -C /opt/graalvm --strip-components=1 \
 && rm /tmp/graalvm.tar.gz \
 && /opt/graalvm/bin/java -version \
 && /opt/graalvm/bin/native-image --version
ENV JAVA_HOME=/opt/graalvm \
    GRAALVM_HOME=/opt/graalvm \
    PATH="/opt/graalvm/bin:${PATH}"

# dlcdn is the Apache CDN and keeps current releases; archive.apache.org is the
# rate-limited cold storage that has everything. Try fast first.
RUN ${CH_CURL} -o /tmp/maven.tar.gz \
      "https://dlcdn.apache.org/maven/maven-3/${MAVEN_VERSION}/binaries/apache-maven-${MAVEN_VERSION}-bin.tar.gz" \
 || ${CH_CURL} -o /tmp/maven.tar.gz \
      "https://archive.apache.org/dist/maven/maven-3/${MAVEN_VERSION}/binaries/apache-maven-${MAVEN_VERSION}-bin.tar.gz"
RUN tar -xzf /tmp/maven.tar.gz -C /opt && rm /tmp/maven.tar.gz \
 && ln -s "/opt/apache-maven-${MAVEN_VERSION}" /opt/maven \
 && /opt/maven/bin/mvn -v
ENV MAVEN_HOME=/opt/maven \
    PATH="/opt/maven/bin:${PATH}"

RUN ${CH_CURL} -o /tmp/gradle.zip \
      "https://services.gradle.org/distributions/gradle-${GRADLE_VERSION}-bin.zip" \
 && unzip -q /tmp/gradle.zip -d /opt && rm /tmp/gradle.zip \
 && ln -s "/opt/gradle-${GRADLE_VERSION}" /opt/gradle \
 && /opt/gradle/bin/gradle -v
ENV GRADLE_HOME=/opt/gradle \
    PATH="/opt/gradle/bin:${PATH}"

RUN ${CH_CURL} -o /tmp/kotlin.zip \
      "https://github.com/JetBrains/kotlin/releases/download/v${KOTLIN_VERSION}/kotlin-compiler-${KOTLIN_VERSION}.zip" \
 && unzip -q /tmp/kotlin.zip -d /opt && rm /tmp/kotlin.zip \
 && /opt/kotlinc/bin/kotlinc -version
ENV PATH="/opt/kotlinc/bin:${PATH}"

# Eclipse JDT language server, for the LSP tool on .java files. The snapshot
# URL is stable; the launcher is the python script shipped in the tarball.
RUN mkdir -p /opt/jdtls \
 && ${CH_CURL} -o /tmp/jdtls.tar.gz \
      "https://download.eclipse.org/jdtls/snapshots/jdt-language-server-latest.tar.gz" \
 && tar -xzf /tmp/jdtls.tar.gz -C /opt/jdtls && rm /tmp/jdtls.tar.gz \
 && ln -s /opt/jdtls/bin/jdtls /usr/local/bin/jdtls \
 && test -x /opt/jdtls/bin/jdtls
