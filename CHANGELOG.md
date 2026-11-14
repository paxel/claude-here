# Changelog

## 0.5.0

* Release archives and `install.sh` carry signed SLSA build provenance; `gh attestation verify <file> --repo paxel/claude-here` checks a download ([here-1014](https://github.com/paxel/claude-here/issues/22)).
* The base image starts from a fixed Debian `trixie-slim` digest and upgrades its packages while building, so `update --base` still brings Debian's security fixes. `images/` changed, so every image is rebuilt once on first start ([here-1013](https://github.com/paxel/claude-here/issues/21)).
* Internal: the Rust toolchain is pinned to 1.100.0 ([here-1000](https://github.com/paxel/claude-here/issues/6)).
* Internal: tests, integration tests and the dependency audit run as separate CI jobs on every pull request ([here-1001](https://github.com/paxel/claude-here/issues/7)); the audit also fails on unsound, unmaintained and yanked crates, and rustix is 1.1.6 ([here-1012](https://github.com/paxel/claude-here/issues/20)).
* Internal: a Quality workflow checks formatting, clippy and a coverage floor ([here-1002](https://github.com/paxel/claude-here/issues/8)); SonarQube Cloud analyses main ([here-1003](https://github.com/paxel/claude-here/issues/9)); the network report views have unit tests ([here-1011](https://github.com/paxel/claude-here/issues/19)).
* Internal: CodeQL ([here-1004](https://github.com/paxel/claude-here/issues/10)), OpenSSF Scorecard ([here-1005](https://github.com/paxel/claude-here/issues/11)) and fuzzing of the parsers ([here-1015](https://github.com/paxel/claude-here/issues/23)).
* Internal: Dependabot for cargo, GitHub Actions and the base image ([here-1006](https://github.com/paxel/claude-here/issues/12)); actions pinned to commit SHAs with read-only tokens ([here-1007](https://github.com/paxel/claude-here/issues/13)).
* Internal: a security policy with private vulnerability reporting ([here-1008](https://github.com/paxel/claude-here/issues/14)), status badges ([here-1009](https://github.com/paxel/claude-here/issues/15)) and a protected main branch ([here-1010](https://github.com/paxel/claude-here/issues/16)).

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
