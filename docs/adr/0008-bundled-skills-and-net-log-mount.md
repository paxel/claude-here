# ADR 0008: Bundled skills synced by the entrypoint, summaries-only net log mount

Date: 2026-09-19
Status: accepted

## Context

Two things the sandbox could tell Claude, and one it could show it:

* which toolchains and language servers exist in this image,
* how to finish work when committing is impossible (`ro` mode),
* what the session actually talked to.

The container `~/.claude` is a mount (`~/.config/claude_here/home`), not part of
the image, so an image cannot place a skill where Claude looks for one. And the
capture directory that would answer the third point lives on the host and is
not mounted at all.

## Decision

**Facts go in the channel that already exists.** The toolchain list, the cloud
mode, the net mode and the granted MCP servers are fields of
`CLAUDE_HERE_SESSION_INFO` and one sentence of the appended system prompt.
Facts do not become skills: the prompt is always present and costs no tool
call, a skill has to be found and loaded.

**Procedures are shipped as skills, synced by the root phase.** The image
carries them under `/usr/local/lib/claude_here/skills/`; the entrypoint copies
them into `$HOME/.claude/skills/claude_here/` on every start, so they always
match the binary instead of going stale until the next `init --reseed`. The
same unit carries the `lspServers` declarations for the shipped toolchains,
including `dart language-server`, for which no official plugin exists — so no
plugin has to be fetched from a marketplace inside the sandbox, which would
require network and git and would fail under an egress allowlist.

**The net log mount is summaries only.** A per-run directory is assembled that
contains the `.summary.json` files of the current project and nothing else,
mounted read-only.

## Consequences

* The pcap files are never visible to the container. They are captured with
  `-s 512`, which includes HTTP headers, so mounting them would hand Claude the
  `Authorization` headers of every past plain-HTTP request in every project.
  Cross-project host history stays invisible for the same reason.
* Residual exposure remains: a summary lists plain-HTTP URLs, and a URL can
  carry a token in its query string.
* The claim in the README that the sandbox user "cannot stop it or read it"
  becomes "cannot stop it or tamper with it", plus an explicit statement of
  what the container can read.
* The current session's own summary does not exist until the session ends, so
  self-triage covers earlier sessions of the same project.
