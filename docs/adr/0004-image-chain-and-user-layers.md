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

## Amendment 2026-09-25: Claude Code is the last layer, parents hashed by id

Claude Code sat in `base`, the bottom of every chain. A new Claude release,
which arrives several times a week, therefore rebuilt the whole chain, and
`update` refreshed only the chain of the current config: the base was rebuilt
under the same hash label, so every other chain (`--node` runs, other
projects) still counted as current and kept running the old Claude.

Decision:

```
claude_here:base-u<uid>  →  ...-<toolchain>...  →  ...-user  →  ...-user-<sha8(project path)>  →  ...-claude
```

* Claude Code is installed by its own layer (`images/claude.dockerfile`) on
  top of everything else, including the user and project layers. A new release
  rebuilds that layer alone. User and project Dockerfiles can no longer call
  `claude` during the build.
* The layer's build arg and hash carry an exact version resolved on the host
  from `downloads.claude.ai/claude-code-releases/<channel>`, the source
  `install.sh` uses. The version is cached daily and refreshed after a session,
  like the tool's own update check. A newer version is offered with a y/N
  prompt at start; a "y" records it as accepted, and every chain rebuilds its
  Claude layer on its next start without asking again. New chains get the
  accepted version.
* `claude_version = "latest"|"stable"` follows a channel; any other value is a
  pin and switches detection off. `update_check = false` switches off both the
  tool's and Claude's check.
* A layer's hash includes the image id of its parent instead of the parent's
  hash. A parent rebuilt under an unchanged hash (`update --base`, `--rebuild`)
  now makes every child stale in every chain, and each rebuilds lazily.
* `update` fetches the version now, accepts it and rebuilds the Claude layer of
  the current chain; it takes the toolchain flags of a run. `update --base`
  additionally rebuilds the base with `--no-cache --pull`.

This supersedes, above: "`base` embeds … the Claude Code version", "`update`
rebuilds the base with `--no-cache --pull`" and the consequence "Changing …
the Claude version rebuilds the whole chain".

Consequences:

* A Claude update costs seconds per chain instead of minutes.
* The final tag gains a `-claude` suffix; the previous final tag stays in use
  as the layer below it. Content replaced by the one-time rebuild is dangling
  and reclaimed by `docker image prune`.
* BuildKit stamps a new creation time on every build, so any rebuild of a
  lower layer (`--rebuild`, `update --base`) gives it a new id and every chain
  above it rebuilds on its next start, not only the one that asked.
* Under `--no-build`, a chain whose parent was rebuilt fails instead of silently
  running on the old one; a chain whose only change is a newer accepted Claude
  keeps its layer with a note.
