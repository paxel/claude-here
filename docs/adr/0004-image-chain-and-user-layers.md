# ADR 0004: Local image chain with hash-labelled layers

Date: 2026-09-19
Status: accepted

## Context

Containers are ephemeral, so anything installed at runtime is lost. Users need
a place to add tools without editing the shipped Dockerfiles, and images must
not silently go stale.

## Decision

Images are built locally only:

```
claude_here:base  →  claude_here:<variant>  →  claude_here:<variant>-user  →  claude_here:<variant>-user-<sha8(project path)>
```

* `base` embeds the container user name, uid, gid and the Claude Code version;
  they are build args and part of the hash.
* Variants (`rust`, `jvm`) are `FROM claude_here:base`.
* `~/.config/claude_here/Dockerfile` and `.claude_here/Dockerfile` hold plain
  instructions; the tool wraps them with `FROM <parent>`, `USER root` and the
  entrypoint. Images end as root because the entrypoint performs the drop.
* Every image carries `claude_here.hash = sha256(inputs + parent hash)`. A
  layer is rebuilt when the label differs, `--rebuild` forces, `--no-build`
  fails instead. `update` rebuilds the base with `--no-cache --pull`.

## Consequences

* No registry, no CI needed for images; first start builds for a few minutes.
* Changing the user name or Claude version rebuilds the whole chain.
* Custom image names (`--image my/img`) bypass the chain entirely.
