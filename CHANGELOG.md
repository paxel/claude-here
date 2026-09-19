# Changelog

## 0.1.0 (unreleased)

Initial release.

* `claude_here` / `claude_yolo`: run Claude Code in a project-scoped Docker sandbox.
* Git modes `ro` (kernel-enforced read-only `.git`), `commit` (git shim allowlist) and `full`.
* Long-lived token auth via `claude setup-token`, persistent container `~/.claude` seeded from the host.
* Explicit mounts, environment allowlist, host cache mounts for Maven/Gradle/Cargo, JVM trust store wiring.
* Composable toolchains instead of image variants: `rust`, `jvm`, `dart`, `node`, `python`, `go`, `cpp`, `uv`, `android`, `docs`, `k8s`, `terraform`, `aws`, `gcloud`, `azure`, each its own layer in a fixed build order, with its own host caches, language server and egress domains; `android` implies `jvm`, `python` implies `node` and `uv`. `--image` now only selects a custom local image. `claude_here toolchains` lists them.
* Language servers ship with their toolchain (rust-analyzer, jdtls, pyright, gopls, clangd, typescript-language-server, dart), so Claude Code's LSP tool works in the sandbox.
* `graphviz` added to the base image (changes the base hash; rebuild on first run).
* Global and per-project user Dockerfile layers, hash-based rebuilds, `update`.
* Network modes `full` (default) and `allowlist`: in `allowlist` a dnsmasq-fed ipset plus iptables restricts egress to the hosts the enabled toolchains declare and `net_allow` adds; nothing is intercepted, so TLS is untouched. Adds `NET_ADMIN` to the root phase only in that mode.
* Cloud modes `none` (default, no credentials mounted), `ro` (verb-allowlisting shims for kubectl/helm/terraform/aws/gcloud/az) and `full`, published to `/etc/claude_here/cloud_mode` by the root phase; `~/.kube`, `~/.aws`, `~/.config/gcloud` and `~/.azure` are mounted read-only when the matching toolchain is enabled. `claude_yolo --cloud full` needs `--i-know`.
* MCP servers are opt-in per project (`mcp = [...]` / `--mcp NAME`); `mcpServers` is no longer seeded from the host `.claude.json`.
* Per-session network capture inside the container with `tshark` summaries; `net last|show|top|grep|shark|list|prune`.
* TOML configuration (global < project < CLI), `config show|set|path`, `--save`.
* `init`, `build`, `completions` (fish/bash/zsh, installed for fish by `init`), `uninstall`.
* Release workflow producing Linux (musl) and macOS tarballs, `install.sh`, Homebrew formula template.
