# ADR 0006: Egress allowlist through an in-container CONNECT proxy

Date: 2026-09-19
Status: accepted

## Context

Filesystem, git history, host environment and resources are all restricted
selectively. Network was the one axis that was only *observed*: every session
is recorded (ADR 0003), but nothing prevented the container from reaching any
host — fetching a script and running it, installing from an arbitrary index, or
posting the working tree somewhere. Recording is forensics, not restriction.

`--network none` is not an option: Claude Code needs `api.anthropic.com`.
Packet filters cannot express the rule either — registry IPs rotate, and the
TLS SNI an `iptables` rule would need is not available to it.

## Decision

A `net_mode` axis, default `full`:

* `full` — today's behaviour, unrestricted egress, still recorded.
* `allowlist` — all egress is dropped except DNS and a proxy listening inside
  the container. The proxy accepts `CONNECT` and matches the requested host
  against the allowlist; nothing is intercepted or re-signed, so no certificate
  has to be injected and no client has to trust anything new. It runs as its
  own unprivileged user, like the `netlog` user of ADR 0003, and the sandbox
  user cannot reconfigure it.

The root phase needs `NET_ADMIN` to install the filter rules. The sandbox user
still holds no capabilities (unprivileged uid, `no-new-privileges`), so this
does not widen what Claude can do.

The allowlist is assembled from three sources:

* a fixed minimum required for Claude Code to function at all,
* the domains declared by each enabled toolchain (ADR 0004) — `--rust` brings
  `crates.io`, `--jvm` brings `repo.maven.apache.org`, and so on, so enabling a
  toolchain is enough for its package manager to work,
* `net_allow` entries from the configuration.

Building a project-specific list is empirical and needs no guessing: run once
in `full`, read `claude_here net top`, promote the hosts that are acceptable.

## Consequences

* `WebFetch` fetches client-side and therefore fails for hosts that are not on
  the list. There is deliberately no bypass — a bypass would be a hole through
  the axis. The proxy's denial names the blocked host so it can be added.
* `claude plugin install` and marketplace updates need `github.com` on the list.
* Cloud modes (ADR 0005) interact badly with a static list: a cluster API
  endpoint comes from the mounted kubeconfig, not from a known domain, so it
  has to be derived at run time from the mounted credentials.
* A failed build under `allowlist` must be diagnosable; the blocked host is
  reported in the session summary, not only in the proxy log.
* The default stays `full` for now. Flipping the default to `allowlist` is a
  later decision, once the shipped toolchain domain lists have proven complete.
