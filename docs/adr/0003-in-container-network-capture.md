# ADR 0003: Network recording inside the container, no proxy, no sidecar

Date: 2026-09-19
Status: accepted

## Context

An overview of what the agent talked to is wanted per session and over time.
An allowlisting proxy (copilot_here's Airlock) means TLS interception, a CA in
the image and per-tool proxy configuration. A sidecar container sharing the
network namespace adds a second container to manage. Both were rejected.

## Decision

The image entrypoint runs as root, starts `tcpdump -i any -s 512 -Z netlog`
(tcpdump drops to the unprivileged `netlog` user itself), then drops to the
sandbox user with `setpriv` and runs Claude. The sandbox user cannot signal
the capture process nor read its directory. On exit the root parent stops the
capture, runs `tshark` to produce a JSON summary (DNS queries, TLS SNI,
endpoints, connection counts, bytes per direction, plain HTTP requests) and
copies pcap + summary into the host log directory. Retention is time based.

Capabilities are `--cap-drop ALL` plus `NET_RAW SETUID SETGID SETPCAP KILL
CHOWN DAC_OVERRIDE FOWNER` for the root phase, with `no-new-privileges`, so
file capabilities are not usable — which is why the capture is started by root
rather than via `setcap`.

## Consequences

* Observability only. No enforcement in v1; an allowlist would be nftables in
  the same namespace, not a proxy.
* Host networking (`--network host` via `docker_args`) disables the capture.
* Snaplen 512 keeps captures small; byte totals use on-wire frame lengths.
* `docker --init` provides PID 1; the entrypoint is the root parent of the
  session.
