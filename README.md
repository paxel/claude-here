# claude_here

Run [Claude Code](https://docs.anthropic.com/en/docs/claude-code) inside a
project-scoped Docker sandbox — with git modes that are actually enforced, an
explicit mount and environment allowlist, and a per-session network recording
you can inspect afterwards.

Inspired by Gordon Beeming's
[copilot_here](https://github.com/GordonBeeming/copilot_here), which does the
same for GitHub Copilot CLI.

```
cd ~/projects/foo
claude_here                      # interactive Claude Code, sandboxed
claude_here -p "explain main.rs" # anything not a claude_here flag goes to claude
claude_yolo                      # --dangerously-skip-permissions, still sandboxed
```

## Why

* **The container sees only the current directory.** Nothing else from your
  home is visible unless you mount it.
* **Git is enforced, not requested.** In the default `ro` mode every `.git`
  directory is bind-mounted read-only by the kernel. Claude cannot commit,
  checkout, switch branches, stash, reset or push — no matter what it decides
  to try. You review the diff and commit on the host.
* **No credentials leak.** Auth uses a long-lived token obtained once with
  `claude setup-token`; ssh agent and `gh` config are only mounted when you ask
  for them in `full` git mode. Host environment variables are passed only when
  named explicitly.
* **Network is recorded.** A `tcpdump` owned by a separate user runs inside the
  container; the sandbox user cannot stop it or read it. After the session a
  `tshark` summary tells you which hosts were contacted, how often and how much
  data went each way. `claude_here net last|top|grep|shark` reads it back.
* **Non-root, no sudo.** The container user (default `ni`) is created with
  the uid/gid of the invoking host user, so every file it writes is yours;
  the image chain is per host user (`claude_here:<variant>-u<uid>`) and the
  entrypoint refuses to start on a uid mismatch. It has no way up. All capabilities except the handful the root
  entrypoint needs are dropped, `no-new-privileges` is set.
* **Ephemeral containers, persistent state where it matters.** Each run is a
  fresh `--rm` container. Claude's own state (`~/.claude`: sessions, memory,
  plugins) lives in `~/.config/claude_here/home/` and persists.

## Install

Requirements: Docker (or a compatible CLI set via `CLAUDE_HERE_DOCKER`), your
user in the `docker` group, Claude Code installed on the host for the one-time
`claude setup-token`.

```
# from source
cargo install --git https://github.com/paxel/claude-here

# prebuilt binary (Linux x86_64/aarch64, macOS arm64/x86_64) into ~/.local/bin — available once v0.1.0 is tagged
curl -fsSL https://github.com/paxel/claude-here/releases/latest/download/install.sh | sh

# Homebrew — available once the formula is published to paxel/homebrew-tap
brew tap paxel/tap && brew install claude-here

claude_here init
```

`init` creates `~/.config/claude_here/` (config, user Dockerfile, container
home), runs `claude setup-token`, seeds the container home from your host
`~/.claude` (`settings.json`, `CLAUDE.md`, `skills/`, `commands/`, `plugins/`),
installs fish completions and offers to add `.claude_here/` to your global git
ignore. The first `claude_here` builds the `base` image locally (a few minutes).

## Usage

```
claude_here [claude_here flags] [claude arguments...]
claude_here [claude_here flags] -- [claude arguments...]
claude_yolo ...                                  # same, skips permission prompts
claude_here init|build|update|config|net|completions|uninstall
```

Everything that is not a `claude_here` flag is forwarded verbatim to `claude`
inside the container (`-p`, `--resume`, `--model`, `-c`, …). `--` forces
forwarding. `--help`/`--version` outside `--` refer to `claude_here`.

### Flags

| Flag | Meaning |
|------|---------|
| `--image NAME` | Variant `base` (default), `rust`, `jvm`, or any local image name |
| `--mount PATH`, `--mount-rw PATH` | Extra bind mount, read-only / read-write. `PATH` or `host:container`. Repeatable |
| `--env KEY=VALUE`, `--env KEY` | Literal value, or pass `KEY` through from the host. Repeatable |
| `--git ro\|commit\|full` | Git mode (see below). Default `ro` |
| `--ssh`, `--gh` | Forward ssh agent / mount gh config read-only. `full` mode only |
| `--docker-arg ARG` | Raw `docker run` argument. Repeatable |
| `--memory 8g`, `--cpus 4` | Resource limits (none by default) |
| `--no-net-log` | Skip network capture for this run |
| `--rebuild`, `--no-build` | Force image rebuild / fail instead of building |
| `--save`, `--save-global` | Persist the given flags into project / global config |
| `--i-know` | Required for `claude_yolo` together with `--git full` |
| `--dry-run` | Print the `docker run` command instead of running it |

### Git modes

| Mode | Enforcement | What Claude can do |
|------|-------------|--------------------|
| `ro` (default) | Kernel: `.git` (and worktree/submodule git dirs) bind-mounted read-only | inspect: `status`, `diff`, `log`, `blame`, … Nothing that writes |
| `commit` | `git` wrapper in `PATH` with an allowlist | inspect, `add`, `rm`, `mv`, `commit`, `restore`, `fetch`; read-only `branch`/`tag`/`remote`/`config`. Denied: `push`, `checkout`, `switch`, branch/tag creation, `reset`, `rebase`, `merge`, `stash`, config writes, … |
| `full` | none | everything; `--ssh` / `--gh` become available |

`commit` mode is a guard rail, not a security boundary: a determined process
can call `/usr/bin/git` or write into `.git/` directly. Only `ro` is airtight.
The active mode is printed at every start and also told to Claude through the
system prompt so it does not waste turns trying.

If the current directory is a subdirectory of a repository, the repository's
`.git` is outside the mount; Claude sees plain files without git. Nested
repositories below the current directory are protected as well.

### Paths inside the container

The working directory is mounted at the same path with your host home prefix
replaced by the container home: `/home/you/src/foo` → `/home/ni/src/foo`.
Set `user = "you"` in the config to get identical paths (the image is rebuilt
with that user name).

### Configuration

Two TOML files, project overrides global, CLI flags override both. Lists
(`env`, `mounts`, `docker_args`) are concatenated in that order.

* global: `~/.config/claude_here/config.toml` (commented template written by `init`)
* project: `.claude_here/config.toml`

```toml
image = "rust"
git_mode = "commit"
env = ["EDITOR=nano", "GITHUB_TOKEN"]

[[mounts]]
path = "~/shared-notes"
mode = "ro"

[caches]
isolated = false          # true: private caches under ~/.config/claude_here/cache
m2_exclude = ["settings.xml"]

[tls]
truststore = "~/certs/cacerts"   # wired into JAVA_TOOL_OPTIONS
```

`claude_here config show|set KEY VALUE [--global]|path`, or append `--save`
to any run.

### Images

All images are built locally; nothing is pulled from a registry except the
Debian base. Tags carry the host uid (`claude_here:rust-u1000`) because the
user, uid and gid are baked in.

| Variant | Adds |
|---------|------|
| `base` | Debian trixie slim, Claude Code (native installer), git, curl, ripgrep, fd, jq, python3 + pip/venv, build-essential, pkg-config, libssl-dev, openssh-client, tcpdump, tshark, termshark |
| `rust` | rustup stable with clippy and rustfmt; host `~/.cargo/registry` and `~/.cargo/git` mounted |
| `jvm` | GraalVM CE 21 (JDK + native-image), Maven, Gradle, kotlinc; host `~/.m2` and `~/.gradle` mounted |

Layers on top of the variant:

* `~/.config/claude_here/Dockerfile` — global user layer, applied to every
  variant. Plain instructions, no `FROM`, root build context.
* `.claude_here/Dockerfile` — project layer on top of that.

Layers are rebuilt automatically when their content changes (hash stored as an
image label). `claude_here update` rebuilds the base without cache to pick up
a new Claude Code release; pin one with `claude_version = "1.2.3"`.

### Network recording

Every session (unless `--no-net-log` / `net_capture = false` / host
networking) leaves `~/.config/claude_here/logs/net/<session>.pcap`,
`.summary.json` and `.session.json`. Captures are truncated to 512 bytes per
packet (enough for DNS, TLS SNI and HTTP headers; byte counts stay exact) and
pruned after `net_retention_days` (90).

```
claude_here net last                 # hosts of the most recent session
claude_here net show <session>
claude_here net top --days 30 [--project]
claude_here net grep github.com
claude_here net shark [<session>]    # termshark on the pcap, inside the image
claude_here net list | prune
```

TLS payloads are not decrypted; you see host names (SNI/DNS), endpoints,
connection counts and bytes. Plain HTTP requests are listed with method and
URL.

### What Claude is told

`CLAUDE_HERE_SESSION_INFO` (JSON: session id, image, git mode, mounts, …) is
exported inside the container and a short paragraph describing the sandbox and
the git mode is appended to the system prompt.

## Uninstall

```
claude_here uninstall            # removes the claude_here:* images
claude_here uninstall --purge    # also ~/.config/claude_here (token, home, logs)
cargo uninstall claude-here
```

## Development

```
cargo test                      # unit tests (pure logic)
cargo test -- --ignored         # integration tests, need docker and build the base image
cargo clippy --all-targets -- -D warnings
```

Design decisions are recorded in `docs/adr/`.

## License

MIT
