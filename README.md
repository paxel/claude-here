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
  `claude setup-token`; the ssh agent and a `GH_TOKEN` (from the host's
  `gh auth token`) are only passed when you ask for them in `full` git mode. Host environment variables are passed only when
  named explicitly.
* **Network is recorded, and can be restricted.** A `tcpdump` owned by a
  separate user runs inside the container; the sandbox user cannot stop it or
  tamper with it, and only this session's output directory is mounted, so
  earlier captures are unreachable from inside. `--net allowlist` limits egress
  to the hosts the enabled toolchains and your configuration allow. After the session a
  `tshark` summary tells you which hosts were contacted, how often and how much
  data went each way. `claude_here net last|top|grep|shark` reads it back.
* **Non-root, no sudo.** The container user (default `ni`) is created with
  the uid/gid of the invoking host user, so every file it writes is yours;
  the image chain is per host user (`claude_here:base-u<uid>-...`) and the
  entrypoint refuses to start on a uid mismatch. It has no way up. All capabilities except the handful the root
  entrypoint needs are dropped, `no-new-privileges` is set.
* **Ephemeral containers, persistent state where it matters.** Each run is a
  fresh `--rm` container. Claude's own state (`~/.claude`: sessions, memory,
  plugins) lives in `~/.config/claude_here/home/` and persists.

## Install

### What has to be there first

| | Container runtime | Notes |
|---|---|---|
| **Linux** | `docker-ce` (or podman/nerdctl via `CLAUDE_HERE_DOCKER`) | your user in the `docker` group: `sudo usermod -aG docker $USER`, then log out and back in |
| **macOS** | Docker Desktop, Colima or OrbStack | there is no `docker` group; the VM runs as you. Colima: `colima start --cpu 4 --memory 8` |
| **WSL2** | Docker Desktop with WSL integration enabled for your distro, or `docker-ce` installed inside it | see the WSL notes below — they are not optional |

Everywhere: BuildKit (the default for years; the tool sets `DOCKER_BUILDKIT=1`
itself, and the images use `RUN --mount=type=cache`), and Claude Code on the
*same* machine for the one-time `claude setup-token`.

### Then

```sh
# from source — needs a Rust toolchain
cargo install --git https://github.com/paxel/claude-here

# prebuilt binary into ~/.local/bin (Linux x86_64/aarch64, macOS arm64/x86_64)
# — available once v0.1.0 is tagged
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

### WSL

WSL2 only — WSL1 has no namespaces or cgroups and cannot run containers.

**Keep the project inside the WSL filesystem** (`~/src/foo`, not
`/mnt/c/Users/you/src/foo`). A path under `/mnt/c` is a Windows bind mount: it is
slow, and it does not carry Linux uid/gid. This tool bakes your uid/gid into the
image and the entrypoint refuses to start on a mismatch, so a project on the
Windows drive will either fail that check or hand the container files it cannot
own. The same applies to `~/.config/claude_here` — it belongs in the WSL home.

Install claude_here *inside* the distro, not on Windows: it shells out to
`docker`, `git` and `id`, and `claude setup-token` has to write into the WSL home
it will later read from.

Docker Desktop users: enable integration for the distro under *Settings →
Resources → WSL integration*, otherwise `docker` is not on `PATH` there. If you
installed `docker-ce` in the distro instead, start it yourself
(`sudo service docker start`) — systemd is off in WSL unless you enabled it.

### macOS

The container is Linux even on a Mac, which is why **iOS cannot be built in the
sandbox** — Xcode and the iOS SDKs are macOS-only. Claude can edit `ios/`
sources; building and signing stay on the host or in CI.

On Apple Silicon everything runs `arm64` natively; every shipped toolchain has an
`aarch64` build. Give the VM enough memory for the JVM or Flutter toolchains
(8GB is comfortable) — the container limit set by `--memory` cannot exceed what
the VM has.

## Usage

```
claude_here [claude_here flags] [claude arguments...]
claude_here [claude_here flags] -- [claude arguments...]
claude_yolo ...                                  # same, skips permission prompts
claude_here init|build|update|config|net|toolchains|completions|uninstall
```

Everything that is not a `claude_here` flag is forwarded verbatim to `claude`
inside the container (`-p`, `--resume`, `--model`, `-c`, …). `--` forces
forwarding. `--help`/`--version` outside `--` refer to `claude_here`.

### Examples

```sh
# first time on a machine
claude_here init                          # token, config, seeded home, fish completions
cd ~/src/foo && claude_here               # builds claude_here:base-u<uid> once, then starts Claude

# one-shot prompts; everything after the tool flags goes to claude
claude_here -p "summarize the failing tests"
claude_here --resume                      # claude's own flags pass straight through
claude_here -- --help                     # claude's help (without -- it is claude_here's)

# toolchains — any combination, one image layer each
claude_here --rust                        # cargo/clippy/rustfmt/rust-analyzer, ~/.cargo mounted
claude_here --jvm --node                  # GraalVM/Maven/Gradle/kotlinc/jdtls + Node, npm/pnpm/yarn
claude_here --dart --android              # Flutter + Dart, Android SDK (implies --jvm)
claude_here --python                      # poetry/ruff/mypy/pyright (implies --node and --uv)
claude_here --docs                        # plantuml, d2, typst, pandoc
claude_here --k8s --terraform --cloud ro  # cluster and IaC tooling, read verbs only
claude_here toolchains                    # what is available
claude_here --rust --docs --save          # remember for this project (.claude_here/config.toml)
claude_here config set toolchains '["rust"]' --global   # every project gets rust

# git modes
claude_here                               # ro: .git read-only, kernel enforced
claude_here --git commit                  # Claude may `git add` and `git commit`, nothing else
claude_here --git full --ssh --gh         # unrestricted git, ssh agent forwarded, gh config mounted
claude_yolo --git full --i-know           # yolo + full needs an explicit opt-in

# extra mounts and environment
claude_here --mount ~/notes               # read-only at /home/ni/notes
claude_here --mount-rw /srv/data:/data    # read-write at a chosen container path
claude_here --env GITHUB_TOKEN            # pass one host variable through
claude_here --env RUST_LOG=debug          # set a literal
claude_here --docker-arg --add-host --docker-arg db:10.0.0.5   # raw docker flags

# resources, images, debugging
claude_here --memory 8g --cpus 4
claude_here --rebuild                     # rebuild the whole chain now
claude_here update                        # newest Claude Code now: rebuilds only the Claude layer
claude_here update --node                 # the same for the image a --node run uses
claude_here update --base                 # also refresh the OS packages of the base image
claude_here build --toolchain jvm         # pre-build without starting a session
claude_here --dry-run -p x                # print the docker run command and exit
claude_here --image my-dev:latest         # bring your own image (must have claude in PATH)

# what did it talk to?
claude_here net last
claude_here net top --days 30 --project
claude_here net grep pypi.org
claude_here net shark                     # termshark on the last capture
claude_here net prune --days 7
```

A project that always wants the same setup:

```toml
# .claude_here/config.toml
toolchains = ["jvm", "node"]
git_mode = "commit"
env = ["MAVEN_OPTS=-Xmx2g", "SONAR_TOKEN"]

[[mounts]]
path = "~/company/certs"
mode = "ro"

[tls]
truststore = "~/company/certs/cacerts"
```

```dockerfile
# .claude_here/Dockerfile — extra tools for this project only
RUN apt-get update && apt-get install -y --no-install-recommends protobuf-compiler && rm -rf /var/lib/apt/lists/*
RUN npm install -g @anthropic-ai/mcp-inspector
```

### Flags

| Flag | Meaning |
|------|---------|
| `--rust`, `--jvm`, `--dart`, `--node`, `--python`, `--go`, `--cpp`, `--uv`, `--android`, `--docs`, `--k8s`, `--terraform`, `--aws`, `--gcloud`, `--azure` | Enable a toolchain; any combination. `-t NAME` does the same. `claude_here toolchains` lists them |
| `--image NAME` | Run a custom local image instead of the toolchain chain (must have `claude` in `PATH`) |
| `--cloud none\|ro\|full` | Cloud mode (see below). Default `none` |
| `--net full\|allowlist` | Network mode. Default `full` |
| `--net-allow HOST`, `--mcp NAME` | Extra allowed host / granted MCP server. Repeatable |
| `--mount PATH`, `--mount-rw PATH` | Extra bind mount, read-only / read-write. `PATH` or `host:container`. Repeatable |
| `--env KEY=VALUE`, `--env KEY` | Literal value, or pass `KEY` through from the host. Repeatable |
| `--git ro\|commit\|full` | Git mode (see below). Default `ro` |
| `--ssh`, `--gh` | Forward ssh agent / hand the host's `gh auth token` to the container as `GH_TOKEN` (per-run 0600 env-file, never on the command line). `full` mode only |
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
| `full` | none | everything; `--ssh` / `--gh` become available (`gh` is in the image, authenticated only here) |

`commit` mode is a guard rail, not a security boundary: a determined process
can call `/usr/bin/git` or write into `.git/` directly. Only `ro` is airtight.
The active mode is printed at every start and also told to Claude through the
system prompt so it does not waste turns trying.

If the current directory is a subdirectory of a repository, the repository's
`.git` is outside the mount; Claude sees plain files without git. Nested
repositories below the current directory are protected as well.

Configurations written before toolchains existed still load: `node = true`,
`uv = true` and `image = "base"`/`"rust"`/`"jvm"` are translated into the
matching toolchains and reported as obsolete at start. `claude_here config show`
lists what it translated; `claude_here --rust --save-global` writes the new form.

### Cloud modes

Same shape as the git modes, default `none`. Installing `kubectl` is trivial;
deciding what a mounted kubeconfig means is not, because there is no kernel
equivalent for a cluster: a read-only mount of `~/.kube/config` protects the
file, not the account.

| Mode | Enforcement | What Claude can do |
|------|-------------|--------------------|
| `none` (default) | no credentials are mounted at all | write, render and validate: `helm template`, `kustomize build`, `terraform validate`, edit manifests and IaC. No live system is reachable |
| `ro` | shims at the front of `PATH` for `kubectl`, `helm`, `terraform`, `aws`, `gcloud`, `az` | read verbs: `get`, `describe`, `logs`, `top`, `plan`, `template`, `list`… Denied: every mutation, and `kubectl exec`/`port-forward` — arbitrary execution in a pod and a network bridge |
| `full` | none | everything, including destroying infrastructure |

```sh
claude_here --k8s                            # tools, no credentials
claude_here --k8s --cloud ro                 # ~/.kube mounted, read verbs only
claude_here --terraform --aws --cloud full   # unrestricted
claude_yolo --cloud full --i-know            # yolo + full needs the opt-in
```

Credentials need no flags of their own: `~/.kube` is mounted when `k8s` is
enabled and the mode is not `none`, and likewise `~/.aws`, `~/.config/gcloud`
and `~/.azure` with their toolchains. The mode is written by the root phase to
`/etc/claude_here/cloud_mode`, where the sandbox user cannot change it, and it
is stated in the appended system prompt.

Like `commit` git mode, `ro` is a guard rail, not a boundary — the real binary
can still be called by its absolute path. Actual enforcement belongs on the
other side: a kubeconfig context bound to a read-only service account, an IAM
role with a read-only policy. What you do get for free is the audit trail: every
cloud API call shows up in `claude_here net last`.

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
toolchains = ["rust"]
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

```sh
claude_here config show                       # merged file view + effective values
claude_here config set git_mode commit        # project
claude_here config set caches.isolated true --global
claude_here config set env '["EDITOR=nano","GITHUB_TOKEN"]'
claude_here config path
claude_here --rust --git commit --save        # same thing from a run
```

### Images

All images are built locally; nothing is pulled from a registry except the
Debian base. Tags carry the host uid (`claude_here:base-u1000-jvm-rust`) because
the user, uid and gid are baked in.

```
claude_here:base-u<uid>  →  ...-<toolchain>...  →  ...-user  →  ...-user-<sha8(project path)>  →  ...-claude
```

Claude Code is the last layer of every chain, so a new Claude release rebuilds
that one small layer and nothing below it.

Toolchains compose: every one is a layer, they are applied in a fixed order
regardless of the order you type them, and each prefix of a chain is itself a
usable image. A toolchain brings its own host cache mounts, the language server
for its files, and (under `--net allowlist`) the hosts its package manager needs.

The fixed order is **most expensive first**, because a docker layer's cache key
includes everything beneath it: a layer at the bottom is rebuilt only when the
base or its own definition changes, one at the top whenever something is
inserted below it. So `dart` (a ~2.5GB Flutter checkout) is at the bottom and the
single-binary cloud tools are at the top, and adding `--terraform` to an existing
chain costs one small layer instead of re-downloading Flutter. Dependencies
outrank cost, which is why `node` and `uv` sit below `python`, and `android`
above `jvm`. `claude_here toolchains` lists them in that order.

Adding a toolchain that lands *below* what you already have still rebuilds what
is above it — nothing avoids that in a linear chain. If you know the set, put it
in `.claude_here/config.toml` once.

| Toolchain | Adds | Host caches |
|-----------|------|-------------|
| `node` (`npm`, `js`) | Node.js 22, npm, pnpm, yarn, TypeScript, typescript-language-server | `~/.npm` |
| `uv` | uv/uvx — also how most MCP servers are launched | `~/.cache/uv` |
| `python` | poetry, ruff, mypy, pyright (implies `node`, `uv`) | `~/.cache/pip` |
| `jvm` | GraalVM CE 21 (JDK + native-image), Maven, Gradle, kotlinc, jdtls | `~/.m2`, `~/.gradle` |
| `android` | Android command line tools + platform-tools, licences accepted (implies `jvm`) | `~/Android/Sdk` (platforms, build-tools, ndk) |
| `rust` | rustup stable, clippy, rustfmt, rust-analyzer | `~/.cargo/{registry,git}` |
| `go` | Go toolchain, gopls | `~/go/pkg/mod` |
| `cpp` (`c`) | cmake, ninja, gdb, clang, clangd, clang-format, clang-tidy, valgrind, conan | `~/.conan2` |
| `dart` (`flutter`) | Flutter and Dart SDK with the Dart language server | `~/.pub-cache` |
| `docs` | plantuml, d2, typst, pandoc (graphviz is in the base) | — |
| `k8s` (`kubernetes`) | kubectl, helm, kustomize | — |
| `terraform` | terraform | — |
| `aws` | AWS CLI v2 | — |
| `gcloud` | Google Cloud CLI (~1GB) | — |
| `azure` (`az`) | Azure CLI | — |

The base image holds Debian trixie slim, git,
gh, curl, ripgrep, fd, jq, graphviz, python3 + pip/venv, build-essential,
pkg-config, libssl-dev, openssh-client, tcpdump, tshark and termshark.

Two toolchains imply another, because they cannot work without it: `android`
implies `jvm`, and `python` implies `node` and `uv` (pyright is a node program).
`docs` installs a headless JRE only when the image has no `java` yet, so
`--jvm --docs` reuses GraalVM.

On top of the chain:

* `~/.config/claude_here/Dockerfile` — global user layer, applied to every
  image. Plain instructions, no `FROM`, root build context.
* `.claude_here/Dockerfile` — project layer on top of that.

Layers are rebuilt automatically when their content changes (hash stored as an
image label). A layer's hash includes the id of the image below it, so a
rebuilt base or toolchain makes everything above it stale, in every chain: each
one rebuilds on its next start. Package downloads (apt, pip, npm, Go modules)
live in BuildKit cache mounts, so a rebuilt layer re-downloads nothing.

**iOS cannot be built in the sandbox.** Xcode and the iOS SDKs are macOS-only
and the container is Debian even on a Mac. Claude can edit `ios/` sources and
run the Dart side; building and signing stay on a Mac or in CI.

### MCP servers

Every restriction in this tool is enforced against a process: the `git` shim,
the cloud shims, the egress proxy. An MCP server sidesteps all of them, because
the capability arrives over a socket. A GitHub MCP server with a token can push
while git mode is `ro`; a Kubernetes MCP server ignores the `kubectl` shim.

Nothing is therefore inherited. `mcpServers` is not copied from the host
configuration; a server is available only when it is named:

```toml
mcp = ["github", "sentry"]
```

```sh
claude_here --mcp github
```

The servers granted for a run are printed in the startup line and listed in
`CLAUDE_HERE_SESSION_INFO`. An MCP server can grant reach that no shim here can
restrict — that is why it takes an explicit grant.

### Network modes

Recording tells you what happened; it does not prevent it. `--net allowlist`
restricts egress to the hosts the enabled toolchains need plus whatever you add:

```sh
claude_here --rust --net allowlist                      # crates.io and the API, nothing else
claude_here --net allowlist --net-allow example.com     # plus one host
claude_here config set net_mode allowlist               # per project
```

```toml
net_mode = "allowlist"
net_allow = ["internal.registry.example.com"]
```

How it works: `dnsmasq` becomes the container resolver and puts every address it
answers for an allowed name — subdomains included — into an ipset; a packet
filter accepts destinations in that set and rejects the rest immediately with
`icmp-admin-prohibited`. Nothing is intercepted and no certificate is injected,
so TLS stays end to end, and unlike a proxy this also restricts clients that
ignore `http_proxy` (Maven and Gradle among them). The root phase needs
`NET_ADMIN` to install the rules; the sandbox user still holds no capabilities.

Each toolchain brings the hosts its package manager needs, so `--rust --net
allowlist` can fetch crates without any configuration. Building a project list is
empirical: run once with `--net full`, read `claude_here net top`, and allow what
you accept.

Two consequences worth knowing: Claude's `WebFetch` fails for hosts that are not
allowed — there is deliberately no bypass — and `claude plugin install` needs
`github.com` on the list.

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
URL. Example:

```
$ claude_here net last
session  20260919-095014-8865  (2026-09-19 09:50 UTC)
project  /home/axel/src/foo
image    claude_here:base-u1000-claude   git ro
traffic  516 packets, 624.2 KB up / 189.9 KB down, 5 dns queries
┌───────────────────┬───────────────────┬───────┬──────────┬──────────┐
│ host              ┆ ip:port           ┆ conns ┆ up       ┆ down     │
╞═══════════════════╪═══════════════════╪═══════╪══════════╪══════════╡
│ api.anthropic.com ┆ 160.79.104.10:443 ┆ 7     ┆ 624.2 KB ┆ 189.9 KB │
└───────────────────┴───────────────────┴───────┴──────────┴──────────┘
pcap     /home/axel/.config/claude_here/logs/net/20260919-095014-8865.pcap
```

Every session also prints one line at exit
(`claude_here: net: 1 host(s), 7 connection(s), 624.2 KB up / 189.9 KB down`).

### What Claude is told

`CLAUDE_HERE_SESSION_INFO` is exported inside the container:

```json
{"tool":"claude_here","version":"0.2.0","session_id":"20260919-095014-8865",
 "image":"claude_here:base-u1000-claude","toolchains":[],"mcp":[],"git_mode":"ro","cloud_mode":"none","net_mode":"full","yolo":false,"user":"ni",
 "cwd_host":"/home/axel/src/foo","cwd":"/home/ni/src/foo",
 "mounts":[{"host":"/home/axel/src/foo","container":"/home/ni/src/foo","mode":"rw"},
           {"host":"/home/axel/src/foo/.git","container":"/home/ni/src/foo/.git","mode":"ro"}],
 "net_capture":true,"ssh":false,"gh":false}
```

and this paragraph is passed to `claude` as `--append-system-prompt` (visible
in full with `claude_here --dry-run`; it is appended, Claude's own system
prompt stays intact):

> You are running inside a claude_here Docker sandbox (session `<id>`).
> Container user '`<user>`' has no sudo and no root. The host working directory
> `<host cwd>` is mounted at `<container cwd>`; host home paths appear under
> `/home/<user>`. Only the mounted paths listed in `CLAUDE_HERE_SESSION_INFO`
> exist here.
>
> *ro:* Git mode is 'ro': every .git directory is bind-mounted read-only and
> enforced by the kernel. Committing, staging, checking out, switching
> branches, stashing, resetting and pushing are impossible; do not attempt
> them, do not try to work around this. Report changed files and let the user
> commit.
>
> *commit:* Git mode is 'commit': you may inspect, stage and commit locally.
> Checkout, switch, branch creation, reset, rebase, merge, stash, tag and any
> remote operation are denied by the git wrapper; do not attempt them or
> bypass the wrapper.
>
> *full:* Git mode is 'full': git is unrestricted.
>
> *(when recording)* All network traffic of this session is recorded for later
> review.

Everything else Claude reports about the sandbox (missing toolchains, empty
memory, …) comes from its own probing; nothing is scripted. The text lives in
`src/session.rs` (`SessionInfo::system_prompt`).

Startup prints the effective setup:

```
claude_here: session 20260919-095014-8865 | image claude_here:base-u1000-claude | git ro (1 .git dir(s) read-only) | cloud none | net recorded
```

## Updating

Three things can be out of date, and they update separately:

```sh
claude_here update          # Claude Code: fetch the newest release, rebuild the Claude layer
claude_here update --node   # same, for the image a `--node` run uses
claude_here update --base   # also the OS packages of the base (--no-cache --pull)
```

**Claude Code** is checked the same way as the tool below: once a day, after a
session, cached in `~/.config/claude_here/claude-version.json`. The version comes
from where Claude's own `install.sh` reads it
(`https://downloads.claude.ai/claude-code-releases/latest`). When a newer
release is known, the next interactive start asks:

```
claude_here: Claude Code 2.1.274 -> 2.1.282 available. Update now? [y/N]
```

"y" records the version as accepted: this session's image rebuilds its Claude
layer (seconds), and every other image does the same on its next start without
asking again. "n" keeps the accepted version and asks again next time. A new
image is built with the accepted version. `claude_version = "stable"` follows
the stable channel instead; `claude_version = "2.1.274"` pins that version and
switches the check off. `update` fetches right away and accepts without a
question, even with `update_check = false`, because you asked for it.

After `update --base`, every other image rebuilds on its next start, since the
base below it changed. `--rebuild` has the same effect on other images that
share layers with the one it rebuilds.

For the tool itself, claude_here checks GitHub for a newer release **at most
once a day, after a session has ended** — never while one starts, so nothing is
fetched before your work and nothing is added to the startup time. The answer is
cached in `~/.config/claude_here/update-check.json`.

When the cached answer says a newer release exists, the next interactive start
asks:

```
claude_here: update available: 0.1.0 -> 0.2.0. Update now? [y/N]
```

Answering yes runs the command that matches how the binary was installed — the
path decides: `cargo install --force` under `~/.cargo/bin`, `brew upgrade` under
a Homebrew prefix, otherwise the release `install.sh` — and then restarts with
your original arguments. Answering no prints that command and continues.

There is no prompt without a terminal, so `-p`, pipes and CI only see a single
notice line and never stall. `update_check = false` or
`CLAUDE_HERE_NO_UPDATE_CHECK=1` switches off both checks, the tool's and
Claude Code's.

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
