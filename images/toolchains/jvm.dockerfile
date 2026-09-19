# jvm toolchain: GraalVM CE 21 (JDK + native-image), Maven, Gradle, Kotlin,
# and the Eclipse JDT language server.
#
# Every download is retried and written to a file before it is unpacked: a
# retried pipe would hand tar a restarted stream, and one DNS blip must not
# throw away the rest of a long build.
ARG BASE=claude_here:base
FROM ${BASE}
ARG GRAALVM_VERSION=21.0.2
ARG MAVEN_VERSION=3.9.9
ARG GRADLE_VERSION=8.14
ARG KOTLIN_VERSION=2.1.20
ENV CH_CURL="curl -fsSL --retry 5 --retry-delay 3 --retry-all-errors --connect-timeout 20"

RUN arch="$(dpkg --print-architecture)"; \
    case "$arch" in amd64) ga=x64 ;; arm64) ga=aarch64 ;; *) echo "unsupported arch $arch"; exit 1 ;; esac; \
    mkdir -p /opt/graalvm \
 && ${CH_CURL} -o /tmp/graalvm.tar.gz \
      "https://github.com/graalvm/graalvm-ce-builds/releases/download/jdk-${GRAALVM_VERSION}/graalvm-community-jdk-${GRAALVM_VERSION}_linux-${ga}_bin.tar.gz" \
 && tar -xzf /tmp/graalvm.tar.gz -C /opt/graalvm --strip-components=1 \
 && rm /tmp/graalvm.tar.gz
ENV JAVA_HOME=/opt/graalvm \
    GRAALVM_HOME=/opt/graalvm \
    PATH="/opt/graalvm/bin:${PATH}"

RUN ${CH_CURL} -o /tmp/maven.tar.gz \
      "https://archive.apache.org/dist/maven/maven-3/${MAVEN_VERSION}/binaries/apache-maven-${MAVEN_VERSION}-bin.tar.gz" \
 && tar -xzf /tmp/maven.tar.gz -C /opt && rm /tmp/maven.tar.gz \
 && ln -s "/opt/apache-maven-${MAVEN_VERSION}" /opt/maven
ENV MAVEN_HOME=/opt/maven \
    PATH="/opt/maven/bin:${PATH}"

RUN ${CH_CURL} -o /tmp/gradle.zip \
      "https://services.gradle.org/distributions/gradle-${GRADLE_VERSION}-bin.zip" \
 && unzip -q /tmp/gradle.zip -d /opt && rm /tmp/gradle.zip \
 && ln -s "/opt/gradle-${GRADLE_VERSION}" /opt/gradle
ENV GRADLE_HOME=/opt/gradle \
    PATH="/opt/gradle/bin:${PATH}"

RUN ${CH_CURL} -o /tmp/kotlin.zip \
      "https://github.com/JetBrains/kotlin/releases/download/v${KOTLIN_VERSION}/kotlin-compiler-${KOTLIN_VERSION}.zip" \
 && unzip -q /tmp/kotlin.zip -d /opt && rm /tmp/kotlin.zip
ENV PATH="/opt/kotlinc/bin:${PATH}"

# Eclipse JDT language server, for the LSP tool on .java files. The snapshot
# URL is stable; the launcher is the python script shipped in the tarball.
RUN mkdir -p /opt/jdtls \
 && ${CH_CURL} -o /tmp/jdtls.tar.gz \
      "https://download.eclipse.org/jdtls/snapshots/jdt-language-server-latest.tar.gz" \
 && tar -xzf /tmp/jdtls.tar.gz -C /opt/jdtls && rm /tmp/jdtls.tar.gz \
 && ln -s /opt/jdtls/bin/jdtls /usr/local/bin/jdtls

RUN java -version && native-image --version && mvn -v && gradle -v && kotlinc -version
