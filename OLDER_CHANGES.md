# Older changes

## 0.1.0 (2026-09-19)

Initial release.

* `claude_here` / `claude_yolo`: run Claude Code in a project-scoped Docker sandbox.
* Git modes `ro` (kernel-enforced read-only `.git`), `commit` (git shim allowlist) and `full`.
* Long-lived token auth via `claude setup-token`, persistent container `~/.claude` seeded from the host.
* Explicit mounts, environment allowlist, host cache mounts for Maven/Gradle/Cargo, JVM trust store wiring.
* Composable toolchains instead of image variants: `rust`, `jvm`, `dart`, `node`, `python`, `go`, `cpp`, `uv`, `android`, `docs`, `k8s`, `terraform`, `aws`, `gcloud`, `azure`, each its own layer in a fixed build order, with its own host caches, language server and egress domains; `android` implies `jvm`, `python` implies `node` and `uv`. `--image` now only selects a custom local image. `claude_here toolchains` lists them. Existing configs keep loading: `node = true`, `uv = true` and `image = "base"|"rust"|"jvm"` are translated into toolchains and reported as obsolete at start.
* Language servers ship with their toolchain (rust-analyzer, jdtls, pyright, gopls, clangd, typescript-language-server, dart), so Claude Code's LSP tool works in the sandbox.
* `graphviz` added to the base image (changes the base hash; rebuild on first run).
* Refreshed toolchain versions: Maven 3.9.16 (3.9.9 had left the Apache CDN, so the CDN-first download always fell back to the archive), Kotlin 2.4.20, Go 1.27.1, kubectl 1.37.0, kustomize 5.8.1, Terraform 1.16.3, pandoc 3.11, typst 0.15.1, PlantUML 1.2026.8. Held back on purpose, with the reason recorded next to each pin: Helm 3.x (4.x changes CLI behaviour), Gradle 8.x (9 removed deprecations; projects use their own wrapper), GraalVM 21.0.2 (CE stopped publishing `jdk-21.0.x` tags), Node 22 (LTS into 2027).
* README install section rewritten per platform: Linux, macOS and WSL2, including the WSL pitfalls (WSL1 cannot work, the project must live in the WSL filesystem because `/mnt/c` carries no uid/gid and the image bakes uid/gid).
* Toolchain layers are ordered most-expensive-first, so adding a cheap toolchain no longer rebuilds the expensive ones below it (a docker layer's cache key includes everything beneath it). `dart` is at the bottom, the single-binary cloud tools at the top; dependencies still outrank cost. Chain tags change accordingly, so existing images are rebuilt once.
* Faster image builds: apt lists/archives, pip, npm and the Go module and build caches live in BuildKit cache mounts (the tool sets `DOCKER_BUILDKIT=1`); Flutter and the Android SDK are installed as the sandbox user instead of being `chown -R`'d afterwards, which no longer duplicates the whole tree into a second layer; Maven comes from the Apache CDN with the archive as fallback; `gopls` can be pinned with `GOPLS_VERSION`; the jvm smoke tests moved into the step that installs each tool.
* `claude_here update` no longer rebuilds the whole base: only the Claude Code install step is invalidated (`CLAUDE_REFRESH`), so apt and the toolchain downloads survive.
* Image builds survive transient network failures: every download retries (`curl --retry 5 --retry-all-errors`, apt `Acquire::Retries`, the Flutter clone up to three times), downloads land in a file before being unpacked instead of being piped into tar, and the jvm layer is one step per tool so a failure costs one step rather than the whole layer.
* Global and per-project user Dockerfile layers, hash-based rebuilds, `update`.
* Bundled skills (`ro` handover, diagram render-and-look loop, network self-triage) and a generated plugin declaring the language servers of the enabled toolchains, synced into the container home by the entrypoint on every start.
* Claude can read the network summaries of earlier sessions of the same project under `~/.claude_here/net/`; captures stay host-side, and both the capture output and the summary view are per-session directories, so concurrent runs cannot see or clear each other.
* Network modes `full` (default) and `allowlist`: in `allowlist` a dnsmasq-fed ipset plus iptables restricts egress to the hosts the enabled toolchains declare and `net_allow` adds; nothing is intercepted, so TLS is untouched. Adds `NET_ADMIN` to the root phase only in that mode.
* Cloud modes `none` (default, no credentials mounted), `ro` (verb-allowlisting shims for kubectl/helm/terraform/aws/gcloud/az) and `full`, published to `/etc/claude_here/cloud_mode` by the root phase; `~/.kube`, `~/.aws`, `~/.config/gcloud` and `~/.azure` are mounted read-only when the matching toolchain is enabled. `claude_yolo --cloud full` needs `--i-know`.
* MCP servers are opt-in per project (`mcp = [...]` / `--mcp NAME`); `mcpServers` is no longer seeded from the host `.claude.json`.
* Per-session network capture inside the container with `tshark` summaries; `net last|show|top|grep|shark|list|prune`.
* TOML configuration (global < project < CLI), `config show|set|path`, `--save`.
* Update check for the tool itself: the GitHub release API is consulted at most once a day *after* a session, the answer is cached, and the next interactive start offers to install it with the command matching the install method, then restarts with the original arguments. Off with `update_check = false` or `CLAUDE_HERE_NO_UPDATE_CHECK=1`; never prompts without a terminal.
* `init`, `build`, `completions` (fish/bash/zsh, installed for fish by `init`), `uninstall`.
* Release workflow producing Linux (musl) and macOS tarballs, `install.sh`, Homebrew formula template.
