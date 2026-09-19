# ADR 0006: Egress allowlist through a resolver-fed packet filter

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
* `allowlist` — `dnsmasq` becomes the container resolver and, through
  `ipset=/<domain>/ch_allow`, records every address it answers for an allowed
  name (subdomains included) in an ipset. `iptables` then accepts loopback,
  established connections, the resolver's own traffic and destinations in that
  set, and rejects everything else with `icmp-admin-prohibited` so a blocked
  client fails immediately instead of hanging. IPv6 is filtered the same way and
  rejected wholesale when `ip6tables` is missing: a missing rule must never mean
  "allowed".

  A `CONNECT` proxy was the first design and was dropped: it only restricts
  clients that honour `http_proxy`, and Maven, Gradle and several others do not.
  Feeding the filter from the resolver covers every client, and — like the proxy
  variant — intercepts nothing, so TLS stays end to end and no certificate has
  to be injected anywhere.

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
  the axis. The rejection is immediate and the host appears in the session
  capture, so it can be allowed deliberately.
* `claude plugin install` and marketplace updates need `github.com` on the list.
* Cloud modes (ADR 0005) interact badly with a static list: a cluster API
  endpoint comes from the mounted kubeconfig, not from a known domain, so it
  has to be derived at run time from the mounted credentials.
* A failed build under `allowlist` must be diagnosable: the attempt is in the
  session capture, and the tool prints how many domains are active at start.
* Addresses enter the set only through a lookup that goes to this resolver. A
  client that connects to a hard-coded IP, or reuses an address cached from
  before the session, is rejected.
* The default stays `full` for now. Flipping the default to `allowlist` is a
  later decision, once the shipped toolchain domain lists have proven complete.
