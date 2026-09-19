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
claude_here:base-u<uid>  →  claude_here:<variant>-u<uid>  →  ...-u<uid>-user  →  ...-u<uid>-user-<sha8(project path)>
```

The uid is part of the tag: several host users on one docker daemon each get
an own chain instead of rebuilding each other's images. uid/gid come from the
invoking process (`id -u`/`id -g`), never from directory ownership, and the
entrypoint refuses to start when the image's user does not match.

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
