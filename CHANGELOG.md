# Changelog

## 0.3.0 (unreleased)

* `[tls] ca = "<pem>"` / `--ca <pem>`: a company CA trusted by every client in the container. The PEM goes into the system bundle at session start and the clients with their own root store are pointed at it: Node and Claude Code (`NODE_EXTRA_CA_CERTS`), `pip`/`requests`/`az` (`REQUESTS_CA_BUNDLE`, `PIP_CERT`), `uv` (`UV_NATIVE_TLS`), `aws` (`AWS_CA_BUNDLE`), `gcloud` (`CLOUDSDK_CORE_CUSTOM_CA_CERTS_FILE`), the JDK (`keytool` import into `cacerts`). `[tls] truststore` stays for a pre-built JKS and wins for the JVM when both are set (ADR 0009). `images/entrypoint.sh` changed, so every image is rebuilt once on first start.
* Weekly dependency vulnerability check (`cargo audit` against the RustSec advisory database) as a GitHub Actions workflow, also runnable on demand. It fails only on a known vulnerability; unmaintained or yanked crates are reported but do not fail it.

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
