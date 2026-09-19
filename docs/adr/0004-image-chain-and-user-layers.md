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
claude_here:base-u<uid>  →  claude_here:<variant>-u<uid>  →  ...[-node][-uv]  →  ...-user  →  ...-user-<sha8(project path)>
```

The uid is part of the tag: several host users on one docker daemon each get
an own chain instead of rebuilding each other's images. uid/gid come from the
invoking process (`id -u`/`id -g`), never from directory ownership, and the
entrypoint refuses to start when the image's user does not match.

* `base` embeds the container user name, uid, gid and the Claude Code version;
  they are build args and part of the hash.
* Variants (`rust`, `jvm`) are `FROM claude_here:base`.
* Add-ons (`node`, `uv`) are switches, not variants, so they combine with any
  variant; they form one layer whose tag suffix lists what is enabled.
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

## Amendment 2026-09-19: variants replaced by composable toolchains

`base`/`rust`/`jvm` as mutually exclusive variants could not express projects
that need two toolchains at once (a Flutter app needs Dart *and* a JDK). The
variant/addon split was an accident of ordering, not a design: add-ons already
composed, variants did not.

Variants are therefore gone. The chain is

```
claude_here:base-u<uid>  →  ...-<toolchain>...  →  ...-user  →  ...-user-<sha8(project path)>
```

* Every toolchain is a layer with the same shape as the former add-ons, picked
  by its own flag (`--rust`, `--jvm`, `--dart`, …) or the `toolchains` config
  list. Any combination is allowed.
* Layers are applied in a fixed numeric order defined by the shipped registry,
  never in the order the flags were typed, so a combination always yields one
  tag and one hash. The tag suffix lists the enabled toolchains in that order.
* A toolchain may imply another (`--android` implies `--jvm`); implications are
  resolved before ordering.
* `--image NAME` keeps its second meaning only: a custom local image that
  bypasses the chain. It no longer selects a variant.
* Each toolchain declares its host cache mounts, the egress domains it needs
  (ADR 0006) and the language servers it provides, so enabling it is enough —
  no further configuration.

Consequences:

* Breaking CLI and config change, taken before 0.1.0 is tagged.
* Cache reuse only extends prefixes: `--go` then `--go --jvm` reuses the cached
  `base→go` image, while `--jvm` alone is a different chain and builds again.
* `graphviz` moves into the base image: `dot` is a shared dependency of the
  `docs` toolchain and of several unrelated tools, and it is ~10MB.
