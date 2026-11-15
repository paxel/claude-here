# OpenSSF Best Practices: answers for the passing badge

Prepared answers for the questionnaire at <https://www.bestpractices.dev/en/projects/new>
(project URL `https://github.com/paxel/claude-here`). Each answer names where the evidence is,
so it can be pasted as the justification. "Unmet" marks a criterion the project does not meet
yet; the form accepts that, but the passing badge needs every MUST.

## Basics

| Criterion | Answer | Evidence |
|-----------|--------|----------|
| description_good | Met | README "Why": runs Claude Code inside a project-scoped Docker sandbox |
| interact | Met | GitHub issues and pull requests; private vulnerability reports via the Security tab |
| contribution | Unmet | No contribution guide; the README has a Development section only |
| contribution_requirements | Unmet | Rules exist for agents in `CLAUDE.md`, not for human contributors |
| floss_license | Met | MIT (`LICENSE`) |
| floss_license_osi | Met | MIT is OSI-approved |
| license_location | Met | `LICENSE` at the repository root |
| documentation_basics | Met | README: install, usage, configuration, updating, uninstall; `docs/` |
| documentation_interface | Met | README "Usage" lists every flag and subcommand; `claude_here --help` |
| sites_https | Met | github.com, sonarcloud.io, scorecard.dev; release downloads over HTTPS |
| discussion | Met | GitHub issues, searchable and linkable |
| english | Met | All documentation and issues are in English |
| maintained | Met | Releases 0.2.0 to 0.5.0 within the last months |

## Change control

| Criterion | Answer | Evidence |
|-----------|--------|----------|
| repo_public | Met | <https://github.com/paxel/claude-here> |
| repo_track | Met | git |
| repo_interim | Met | Work lands through pull requests on `release/*` branches |
| repo_distributed | Met | git |
| version_unique | Met | One SemVer version per release, tag `vX.Y.Z`; the release gate checks tag = crate version |
| version_semver | Met | `Cargo.toml` version, SemVer |
| version_tags | Met | `v0.2.0` … `v0.5.0` |
| release_notes | Met | `CHANGELOG.md` (current) and `OLDER_CHANGES.md` (dated history), plus GitHub release notes |
| release_notes_vulns | Met | Fixed advisories are named in the changelog (e.g. RUSTSEC-2026-0486 in 0.5.0) |

## Reporting

| Criterion | Answer | Evidence |
|-----------|--------|----------|
| report_process | Met | GitHub issues |
| report_tracker | Met | GitHub issues, with ticket ids (`docs/agents/ticket-ids.md`) |
| report_responses | Met | Issues are answered and closed with the release that ships the fix |
| enhancement_responses | Met | Enhancement issues are planned into milestones |
| report_archive | Met | GitHub issues are public and kept |
| vulnerability_report_process | Met | `SECURITY.md` |
| vulnerability_report_private | Met | GitHub private vulnerability reporting, enabled |
| vulnerability_report_response | Met | `SECURITY.md`; no report received yet |

## Quality

| Criterion | Answer | Evidence |
|-----------|--------|----------|
| build | Met | `cargo build`; release builds in `.github/workflows/release.yml` |
| build_common_tools | Met | cargo |
| build_floss_tools | Met | Rust toolchain, Docker/Podman |
| test | Met | `cargo test`; docker integration tests in `tests/integration.rs` |
| test_invocation | Met | `cargo test`, documented in README "Development" |
| test_most | Met | Unit tests for every module; coverage floor enforced in `quality.yml` |
| test_continuous_integration | Met | `ci.yml`, `quality.yml`, `fuzz.yml` on every pull request |
| test_policy | Met | `CLAUDE.md`: tests before every commit; coverage ratchet |
| tests_are_added | Met | Every ticket commit carries its tests |
| tests_documented_added | Met | `CLAUDE.md` rules; coverage floor in `quality.yml` |
| warnings | Met | `cargo clippy --all-targets -- -D warnings` (pedantic), CodeQL |
| warnings_fixed | Met | Clippy warnings fail CI |
| warnings_strict | Met | Clippy pedantic, no unwrap/expect/panic in shipped code (`src/lib.rs` lints) |

## Security

| Criterion | Answer | Evidence |
|-----------|--------|----------|
| know_secure_design | Met | ADRs in `docs/adr/` on the sandbox's restrictions (git modes, egress allowlist, cloud modes) |
| know_common_errors | Met | `forbid(unsafe_code)`, panic-free lints, CodeQL, fuzzing |
| crypto_published | N/A | No cryptography of its own; sha2 for content hashes only |
| crypto_call | N/A | Same |
| crypto_floss | N/A | Same |
| crypto_keylength | N/A | Same |
| crypto_working | N/A | Same |
| crypto_weaknesses | N/A | Same |
| crypto_pfs | N/A | No network protocol of its own |
| crypto_password_storage | N/A | Stores no passwords; the Claude token file is 0600 |
| crypto_random | N/A | No security-relevant randomness |
| delivery_mitm | Met | Releases over HTTPS with sha256 files and signed SLSA provenance (`gh attestation verify`) |
| delivery_unsigned | Met | No hash is fetched over HTTP |
| vulnerabilities_fixed_60_days | Met | No open known vulnerability; `cargo audit --deny warnings` on every pull request and weekly |
| vulnerabilities_critical_fixed | Met | Same |
| no_leaked_credentials | Met | Tokens come from the environment or 0600 files outside the repository |

## Analysis

| Criterion | Answer | Evidence |
|-----------|--------|----------|
| static_analysis | Met | CodeQL (`codeql.yml`), clippy, SonarQube Cloud |
| static_analysis_common_vulnerabilities | Met | CodeQL security queries |
| static_analysis_fixed | Met | Code scanning alerts are fixed or dismissed with a reason |
| static_analysis_often | Met | Every pull request and push to main |
| dynamic_analysis | Met | cargo-fuzz targets in `fuzz/`, run by `fuzz.yml` |
| dynamic_analysis_unsafe | N/A | `forbid(unsafe_code)` |
| dynamic_analysis_enable_assertions | Met | The fuzz targets assert round trips and invariants |
| dynamic_analysis_fixed | Met | A fuzz crash fails CI |

## After submitting

The site gives the project an id. The README badge then is
`[![OpenSSF Best Practices](https://www.bestpractices.dev/projects/<id>/badge)](https://www.bestpractices.dev/projects/<id>)`.
