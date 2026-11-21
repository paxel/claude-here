# claude-here

A Rust CLI that runs Claude Code inside a project-scoped Docker sandbox. The terms below are
the ones the code, the README and the ADRs use; `README.md` says how each behaves.

## Language

**Sandbox**:
The container a session runs in: the project directory mounted at the same path as on the
host, the container home, the caches of the enabled toolchains, and nothing else of the host.
_Avoid_: VM, jail, environment

**Session**:
One run of `claude_here` or `claude_yolo`: one container, one session id, one entry under the
network logs. Ends when Claude exits.
_Avoid_: run, job

**Yolo**:
`claude_yolo`: the same sandbox with Claude's permission prompts skipped. A restriction axis
set to `full` under yolo needs `--i-know`.
_Avoid_: unsafe mode, auto mode

**Restriction axis**:
One of the three settings that decide what Claude may reach: the git mode, the cloud mode
and the network mode. The root phase writes each to `/etc/claude_here/`, where the sandbox
user cannot change it, and the system prompt states each.
_Avoid_: permission, policy

**Git mode**:
`ro` (the `.git` directories mounted read-only by the kernel), `commit` (a `git` wrapper with
an allowlist) or `full`. Only `ro` is a boundary; `commit` is a guard rail.

**Cloud mode**:
`none` (no credentials mounted), `ro` (shims that allow read verbs only) or `full`.

**Network mode**:
`full` or `allowlist` (egress restricted to the hosts the enabled toolchains need plus
`net_allow`).

**Network recording**:
The capture of a session's traffic and the summaries built from it (`claude_here net …`).
Off by default.
_Avoid_: net log, pcap

**Toolchain**:
A language or tool set enabled by a flag (`--rust`, `--k8s`, …): one image layer, its caches,
its language server, its egress hosts and the credentials it may mount. Toolchains apply in a
fixed order, whatever order they are typed in.
_Avoid_: image, stack, plugin

**Image chain**:
The images a session's image is built from, each on the one before: base, the toolchains in
their order, the user layer, the project layer and Claude Code last. All built locally.

**User layer**, **project layer**:
The image layers from `~/.config/claude_here/Dockerfile` and from the project's
`.claude_here/Dockerfile`.

**Host facts**:
What `HostFacts::gather` reads from the host before a session: uid and gid, the working
directory, the git and ssh files to mount, the cloud credential directories that exist. `assemble()` turns config and host facts into a run spec without touching
anything itself.
