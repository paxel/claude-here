# claude-here

Rust CLI that runs Claude Code inside a project-scoped Docker sandbox.
Read `README.md` for behaviour and `docs/adr/` for the reasoning behind the
locked design decisions before proposing structural changes.

## Layout

* `src/cli.rs` — clap definitions and the tool-vs-claude argument splitter.
* `src/toolchain.rs` — the shipped toolchain registry: order, implications, caches, egress domains, language servers, credentials.
* `src/plugin.rs` — generates the bundled plugin (skills + `lspServers`) handed to the container.
* `src/config.rs` — TOML layers (global < project < CLI), resolved `Config`.
* `src/run.rs` — pure `assemble()` from config + host facts to `RunSpec`; `execute()`.
* `src/docker.rs` — `RunSpec` → argv, thin docker CLI wrapper.
* `src/image.rs` — image chain, hash labels, user/project layers.
* `src/git.rs` — discovery of `.git` dirs to protect in `ro` mode.
* `src/net/` — session files, summaries, reports.
* `images/` — `Dockerfile.base`, `toolchains/<name>.dockerfile`, `entrypoint.sh`, `git-shim.sh`, `cloud-shim.sh`, `net-allowlist.sh`, `net-summary.sh`, `skills/` (all embedded with `include_str!`).

## Rules

* `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` before every commit.
* No `unwrap`/`expect` outside tests (denied by lints).
* Keep `assemble()` pure; anything touching the filesystem or docker goes into `HostFacts::gather`, `prepare_host_dirs` or `execute`.
* Changing anything under `images/` changes a layer hash and triggers a rebuild for users; mention it in the changelog.
* A new toolchain is a `images/toolchains/<name>.dockerfile` plus one `Toolchain` entry and a flag in `cli.rs`; give it a unique `order` and put implied toolchains before it.
* The three restriction axes (`git_mode`, `cloud_mode`, `net_mode`) are published by the root phase to `/etc/claude_here/*`, never taken from the environment inside the container, and each is stated in `SessionInfo::system_prompt`.
* `cargo test` reads `CLAUDE_CONFIG_DIR` in one init test; run it with that variable unset.
* Image files use BuildKit cache mounts; a file with `RUN --mount` needs `# syntax=docker/dockerfile:1.7` on line 1, and `Docker::run_build` sets `DOCKER_BUILDKIT=1`.
* Never `chown -R` a tree after filling it: own the directory first and write as the target user, otherwise the whole tree is copied into another layer. Tests enforce both rules.
* Commits: one-line summary, no trailers.
