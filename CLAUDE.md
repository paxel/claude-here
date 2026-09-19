# claude-here

Rust CLI that runs Claude Code inside a project-scoped Docker sandbox.
Read `README.md` for behaviour and `docs/adr/` for the reasoning behind the
locked design decisions before proposing structural changes.

## Layout

* `src/cli.rs` — clap definitions and the tool-vs-claude argument splitter.
* `src/config.rs` — TOML layers (global < project < CLI), resolved `Config`.
* `src/run.rs` — pure `assemble()` from config + host facts to `RunSpec`; `execute()`.
* `src/docker.rs` — `RunSpec` → argv, thin docker CLI wrapper.
* `src/image.rs` — image chain, hash labels, user/project layers.
* `src/git.rs` — discovery of `.git` dirs to protect in `ro` mode.
* `src/net/` — session files, summaries, reports.
* `images/` — Dockerfiles, `entrypoint.sh`, `git-shim.sh`, `net-summary.sh` (embedded with `include_str!`).

## Rules

* `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` before every commit.
* No `unwrap`/`expect` outside tests (denied by lints).
* Keep `assemble()` pure; anything touching the filesystem or docker goes into `HostFacts::gather`, `prepare_host_dirs` or `execute`.
* Changing anything under `images/` changes the base hash and triggers a rebuild for users; mention it in the changelog.
* Commits: one-line summary, no trailers.
