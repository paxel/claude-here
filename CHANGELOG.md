# Changelog

## 0.4.1

* Image builds trust the company CA from `[tls] ca`: the first build, `--rebuild` and `update` now work behind a TLS-intercepting proxy. The CA is part of the base image, so changing it rebuilds the chain. `images/` changed, so every image is rebuilt once on first start.
* A CA file with several certificates is imported into the JVM's `cacerts` completely; before, only the first certificate was.

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
