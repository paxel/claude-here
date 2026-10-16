# Changelog

## 0.3.0 (unreleased)

* Weekly dependency vulnerability check (`cargo audit` against the RustSec advisory database) as a GitHub Actions workflow, also runnable on demand. It fails only on a known vulnerability; unmaintained or yanked crates are reported but do not fail it.

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
