# Changelog

## [0.6.1]

* Every release file carries a CycloneDX SBOM of the source and a Sigstore bundle
  `claude_here-<version>-provenance.sigstore.json`, which replaces
  `claude_here-<version>.intoto.jsonl`; the provenance is signed by gates' shared workflow
  ([here-1033](https://github.com/paxel/claude-here/issues/51)).
* The azure toolchain image installs paramiko 5.0.0.
* The image builds and `install.sh` download only over HTTPS and refuse a redirect to plain
  HTTP ([here-1032](https://github.com/paxel/claude-here/issues/50)). `images/` changed, so
  every image is rebuilt once on first start.

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
