# claude-here

Rust CLI that runs Claude Code inside a project-scoped Docker sandbox.
Read `README.md` for behaviour and `docs/adr/` for the reasoning behind the
locked design decisions before proposing structural changes.

## Agent skills

### Issue tracker

Issues live as GitHub issues in `paxel/claude-here`, managed with the `gh` CLI. See
`docs/agents/issue-tracker.md`.

### Triage labels

Default canonical vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`,
`ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `CONTEXT.md` plus `docs/adr/` at the repo root. See `docs/agents/domain.md`.

### Ticket IDs

Prefix `here`, as in `here-1028`. See `docs/agents/ticket-ids.md`.

## Layout

* `src/cli.rs` — clap definitions and the tool-vs-claude argument splitter.
* `src/toolchain.rs` — the shipped toolchain registry: order, implications, caches, egress domains, language servers, credentials, run-shape needs (`RunNeeds`, used by `docker`).
* `src/seccomp.rs` — Docker's default seccomp profile (`images/seccomp-default.json`, verbatim) plus the syscalls a toolchain needs.
* `src/plugin.rs` — generates the bundled plugin (skills + `lspServers`) handed to the container.
* `src/config.rs` — TOML layers (global < project < CLI), resolved `Config`.
* `src/run.rs` — pure `assemble()` from config + host facts to `RunSpec`; `execute()`.
* `src/docker.rs` — `RunSpec` → argv, thin docker CLI wrapper.
* `src/image.rs` — image chain, hash labels, user/project layers.
* `src/git.rs` — discovery of `.git` dirs to protect in `ro` mode.
* `src/net/` — session files, summaries, reports.
* `images/` — `Dockerfile.base`, `toolchains/<name>.dockerfile`, `entrypoint.sh`, `git-shim.sh`, `cloud-shim.sh`, `net-allowlist.sh`, `net-summary.sh`, `seccomp-default.json`, `skills/` (all embedded with `include_str!`).

## Rules

* Before every commit: the checks under Quality gates below.
* No `unwrap`/`expect` outside tests (denied by lints).
* Keep `assemble()` pure; anything touching the filesystem or docker goes into `HostFacts::gather`, `prepare_host_dirs` or `execute`.
* Changing anything under `images/` changes a layer hash and triggers a rebuild for users; mention it in the changelog.
* A new toolchain is a `images/toolchains/<name>.dockerfile` plus one `Toolchain` entry and a flag in `cli.rs`; give it a unique `order` and put implied toolchains before it.
* The three restriction axes (`git_mode`, `cloud_mode`, `net_mode`) are published by the root phase to `/etc/claude_here/*`, never taken from the environment inside the container, and each is stated in `SessionInfo::system_prompt`.
* `cargo test` reads `CLAUDE_CONFIG_DIR` in one init test; run it with that variable unset.
* Image files use BuildKit cache mounts; a file with `RUN --mount` needs `# syntax=docker/dockerfile:1.7` on line 1, and `Docker::run_build` sets `DOCKER_BUILDKIT=1`.
* Never `chown -R` a tree after filling it: own the directory first and write as the target user, otherwise the whole tree is copied into another layer. Tests enforce both rules.
* Commits: one-line summary, no trailers.

## Quality gates

The standard claude-here is held to is [paxel/gates](https://github.com/paxel/gates);
`docs/adr/0011-move-to-gates.md` says where claude-here adds to it and why. The workflows
call gates' shared workflows, pinned by commit; Dependabot moves the pin.

### Before a commit

`cargo fmt`, clippy and the tests, on the toolchain `rust-toolchain.toml` pins. Clippy is two
runs, as in the shared Rust gate: the shipped code under the whole lint set, and the tests
with the lints allowed that are how a test fails:

```sh
cargo clippy --locked --all-features -- -D warnings
cargo clippy --locked --tests --benches --examples --all-features -- -D warnings \
  -A clippy::unwrap_used -A clippy::expect_used -A clippy::panic -A clippy::indexing_slicing \
  -A clippy::string_slice -A clippy::arithmetic_side_effects -A clippy::unreachable
cargo test --locked --all-features
```

A change under `images/` also needs the Docker integration tests, which CI runs after the
Rust gate:

```sh
cargo test --locked --test integration -- --ignored --test-threads=1 docker_toolchain ca_is
```

### Coverage ratchet

Line coverage may only go up. The Rust gate in `ci.yml` fails under the floor in
`coverage-floors.toml`. Every feature ships with tests that hold or raise coverage; a feature
that lowers it is not done. When coverage rises, raise the floor and the badge in the same
pull request, with gates checked out beside the repository:

```sh
cargo tarpaulin --engine llvm --locked --all-features --workspace \
  --skip-clean --out Lcov --output-dir coverage && mv coverage/lcov.info coverage/rust.lcov
python3 ../gates/scripts/coverage_ratchet.py --floors coverage-floors.toml
```

Never lower the floor.

### Fuzzing

`fuzz/` holds the targets `session_id`, `config_file`, `split_args` and `net_files`;
`fuzz.yml` runs them weekly. `fuzz/Cargo.lock` is committed and has its own
`fuzz/osv-scanner.toml`, because osv-scanner reads the exceptions beside the lockfile.

### Before a release

fmt, both clippy runs, the tests, the integration tests, osv-scanner with gates'
`scripts/osv_gate.py` and the coverage ratchet. The version stands in `Cargo.toml` and
`Cargo.lock`. The release workflow runs the shared release gate, the tests and the
integration tests on the tagged commit before it builds.

### Security rules

The decisions behind these are in `docs/adr/0011-move-to-gates.md`.

- Shipped code may not panic: `Cargo.toml` denies unwrap, expect, panic, todo,
  unimplemented, unreachable, indexing, string slicing, overflowing arithmetic and `dbg!`
  under `[lints.clippy]`. Tests are exempt through the second clippy run.
- `unsafe` is forbidden. `#[allow(...)]` is denied: a lint that must be silenced gets
  `#[expect(<lint>, reason = "...")]`.
- Every action in `.github/workflows/` is pinned to a full commit SHA with its version in a
  trailing comment. Never reference an action by tag or branch alone.
- Rust is pinned in `rust-toolchain.toml`; the shared workflows read it from there. The
  workflows' own jobs mirror it (`dtolnay/rust-toolchain@<sha> # <version> branch`). A
  toolchain bump is a commit of its own that changes the file and those references together.
- Every workflow starts with `permissions: contents: read`; a job that writes widens it
  itself.
- A dependency advisory is fixed by an update. When no fixed release exists, it is accepted
  in the `osv-scanner.toml` beside the lockfile with the reason claude-here is not exposed and
  an `ignoreUntil` date. A licence outside the allowlist is accepted per package with
  `effectiveUntil`. Never ignore without reason and date.
- A directory whose manifests are no stack (the image tools' pins) is listed in `gates.toml`
  as `[[NotStacks]]` with its reason; the OSV gate still scans it.
- A code-scanning alert or a SonarQube Cloud finding that stays open is accepted in
  `code-scanning-ignores.toml` with its tool, rule, file, reason and an `ignoreUntil` date.
  Nothing is dismissed in GitHub or marked in SonarQube Cloud.
- The release fails on a known vulnerability, on every open code-scanning alert, secret alert
  and SonarQube Cloud finding that is not accepted, and on a missing `CHANGELOG.md` section for
  the version. The weekly Security run fails on the same, and on unmaintained dependencies and
  passed dates.
- `summer` takes pull requests only.
