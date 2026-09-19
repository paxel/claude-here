# Changelog

## 0.1.0 (unreleased)

Initial release.

* `claude_here` / `claude_yolo`: run Claude Code in a project-scoped Docker sandbox.
* Git modes `ro` (kernel-enforced read-only `.git`), `commit` (git shim allowlist) and `full`.
* Long-lived token auth via `claude setup-token`, persistent container `~/.claude` seeded from the host.
* Explicit mounts, environment allowlist, host cache mounts for Maven/Gradle/Cargo, JVM trust store wiring.
* Image chain `base` / `rust` / `jvm` with global and per-project user Dockerfile layers, hash-based rebuilds, `update`.
* Per-session network capture inside the container with `tshark` summaries; `net last|show|top|grep|shark|list|prune`.
* TOML configuration (global < project < CLI), `config show|set|path`, `--save`.
* `init`, `build`, `completions` (fish/bash/zsh, installed for fish by `init`), `uninstall`.
* Release workflow producing Linux (musl) and macOS tarballs, `install.sh`, Homebrew formula template.
