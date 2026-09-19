# jvm toolchain: GraalVM CE 21 (JDK + native-image), Maven, Gradle, Kotlin,
# and the Eclipse JDT language server.
ARG BASE=claude_here:base
FROM ${BASE}
ARG GRAALVM_VERSION=21.0.2
ARG MAVEN_VERSION=3.9.9
ARG GRADLE_VERSION=8.14
ARG KOTLIN_VERSION=2.1.20

RUN arch="$(dpkg --print-architecture)"; \
    case "$arch" in amd64) ga=x64 ;; arm64) ga=aarch64 ;; *) echo "unsupported arch $arch"; exit 1 ;; esac; \
    mkdir -p /opt/graalvm \
 && curl -fsSL "https://github.com/graalvm/graalvm-ce-builds/releases/download/jdk-${GRAALVM_VERSION}/graalvm-community-jdk-${GRAALVM_VERSION}_linux-${ga}_bin.tar.gz" \
      | tar -xz -C /opt/graalvm --strip-components=1 \
 && curl -fsSL "https://archive.apache.org/dist/maven/maven-3/${MAVEN_VERSION}/binaries/apache-maven-${MAVEN_VERSION}-bin.tar.gz" \
      | tar -xz -C /opt \
 && ln -s "/opt/apache-maven-${MAVEN_VERSION}" /opt/maven \
 && curl -fsSL -o /tmp/gradle.zip "https://services.gradle.org/distributions/gradle-${GRADLE_VERSION}-bin.zip" \
 && unzip -q /tmp/gradle.zip -d /opt && ln -s "/opt/gradle-${GRADLE_VERSION}" /opt/gradle && rm /tmp/gradle.zip \
 && curl -fsSL -o /tmp/kotlin.zip "https://github.com/JetBrains/kotlin/releases/download/v${KOTLIN_VERSION}/kotlin-compiler-${KOTLIN_VERSION}.zip" \
 && unzip -q /tmp/kotlin.zip -d /opt && rm /tmp/kotlin.zip

ENV JAVA_HOME=/opt/graalvm \
    GRAALVM_HOME=/opt/graalvm \
    MAVEN_HOME=/opt/maven \
    GRADLE_HOME=/opt/gradle \
    PATH="/opt/graalvm/bin:/opt/maven/bin:/opt/gradle/bin:/opt/kotlinc/bin:${PATH}"

# Eclipse JDT language server, for the LSP tool on .java files. The snapshot
# URL is stable; the launcher is the python script shipped in the tarball.
RUN mkdir -p /opt/jdtls \
 && curl -fsSL "https://download.eclipse.org/jdtls/snapshots/jdt-language-server-latest.tar.gz" \
      | tar -xz -C /opt/jdtls \
 && ln -s /opt/jdtls/bin/jdtls /usr/local/bin/jdtls

RUN java -version && native-image --version && mvn -v && gradle -v && kotlinc -version
