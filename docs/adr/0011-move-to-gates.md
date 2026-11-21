# ADR 0011: claude-here moves to the gates standard

Date: 2026-11-21
Status: accepted

## Context

claude-here had its own gates: a CI job with the tests, a quality workflow with format,
clippy, a tarpaulin floor and the SonarQube Cloud scan of the main branch, `cargo audit` on
every pull request and weekly, CodeQL, Scorecard, fuzzing on every pull request, and a
release that checked the tag, built, attested and published. Every paxel repository had done
so, each differently. [paxel/gates](https://github.com/paxel/gates) is the one standard that
replaces those, as shared workflows a repository calls, and its audit found claude-here
drifting in 22 checks ([#38](https://github.com/paxel/claude-here/issues/38)).

## Decision

claude-here meets the standard of gates v1.4.0
([here-1028](https://github.com/paxel/claude-here/issues/46) to
[here-1035](https://github.com/paxel/claude-here/issues/53)). Where the standard decides, it
is followed without a note here: the shared OSV gate, Rust gate, SonarQube Cloud job, CodeQL,
Scorecard, fuzz, release gate and provenance workflows, one coverage ratchet, the lint set in
`Cargo.toml`, the files and the badge row. What follows is where claude-here adds to it, or
had a choice.

**The image pins are no stack.** `images/toolchains/node` and `images/toolchains/python` hold
npm lockfiles of the tools the toolchain images install (here-1023), which the audit took for
a TypeScript stack. gates had no way to say otherwise, so it got one first: `[[NotStacks]]` in
a `gates.toml` at the root, with a path and a reason
([gate-1052](https://github.com/paxel/gates/issues/68)). The audit leaves those directories
out of stack detection; the OSV gate still scans their lockfiles, because a vulnerable pinned
tool ships into every sandbox. osv-scanner reads its exceptions beside the lockfile, so the
three directories whose `requirements.txt` holds colorama carry an `osv-scanner.toml` of
their own, as `fuzz/` does for `fuzz/Cargo.lock`. Those files are not embedded in the binary
and change no image. The licence allowlist was written for what a repository links or ships;
claude-here ships only the pins, and pip installs the tools from PyPI on the user's machine.
So the licences of the image tools outside the allowlist (azure-cli's LGPL dependencies,
and those deps.dev cannot name) are accepted per package with that reason and a date,
rather than taking the pins out of the scan.

**Docker integration tests.** The image chain is the product, and only the integration tests
build it on a real daemon, start the entrypoint, start rootless podman inside (ADR 0010) and
check the CA bundle (ADR 0009). The shared Rust gate has no Docker, so `ci.yml` keeps an
`integration` job of claude-here's own after it, and the release runs the same tests on the
tagged commit.

**Release tests.** The shared release gate checks alerts, findings, vulnerabilities, licences
and the changelog section, but runs no tests. A tag can sit on any commit, so the release
keeps a job of its own that checks the tag against the crate version and runs format, both
clippy runs and every test on the tagged commit, followed by the integration job.

**Lints.** The standard's set replaced the `cfg_attr(not(test), deny(...))` blocks of the
library and both binaries. `unsafe_code` is forbidden, which is stricter than the standard's
denial. `clippy::pedantic` stays on top of the set, with the six lints it allowed before. The
two `#[allow]` became `#[expect(..., reason)]`.

**Coverage.** One part, `rust`, measured by the gate with `cargo tarpaulin --engine llvm`. The
first floor is 79.59, the measurement on the CI runner (79.69 %) minus 0.1; a workstation
with Docker measures 80.27 %, because some tests take another branch there. The old floor,
80.46, came from tarpaulin's default engine, which counts lines differently; the tests did
not change. Raise the floor only from a measurement on the runner.

**Fuzzing** runs weekly and by hand only, 900 seconds per target, as in the other
repositories; it no longer runs on every pull request.

**SonarQube Cloud's findings in `images/`.** The first analysis of `summer` found the
downloads of the image builds and of `install.sh` following redirects without restricting
them to HTTPS, and most image downloads have no hash check. Every `curl` there now takes
`--proto =https --tlsv1.2`, which rebuilds every image once. What stays is accepted per rule
and file in `code-scanning-ignores.toml`: the images end as root because the entrypoint's root
phase needs it before it drops to the sandbox user, `go install` at a pinned version, the
literal `http://` the network summary prints, and `[` instead of `[[` in scripts that quote
every operand.

**Release files.** The archives, their `.sha256` files for `install.sh`, and `install.sh`
itself are attested by gates' provenance workflow, which adds a CycloneDX SBOM of the source.
All files of a release share the prefix `claude_here`, the one the archives already had. The
release notes are the changelog section of the version.

## Consequences

- A branch gets its CI run through its pull request; pushes run CI only on `summer`.
- SonarQube Cloud analyses pull requests as well as `summer`, and no longer imports a clippy
  report: clippy fails CI on any warning, so the report was always empty.
- `cargo audit` is gone; osv-scanner reads the same advisories and the licences as well.
- The release and the weekly Security run need the secret `GATES_ALERTS_TOKEN`; the weekly
  run also fails on SonarQube Cloud findings and secret alerts.
- The provenance bundle `claude_here-<tag>.intoto.jsonl` is replaced by
  `claude_here-<tag>-provenance.sigstore.json`, and its signer is gates' provenance workflow.
- The ruleset on `summer` requires the shared gate's checks and `integration`.
