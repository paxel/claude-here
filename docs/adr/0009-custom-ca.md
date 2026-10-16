# ADR 0009: One company CA, trusted by every client in the image

Date: 2026-10-16
Status: accepted

## Context

Company networks terminate TLS at a proxy or run private registries and
artifact servers under a CA of their own. Inside the sandbox that CA is
unknown: the image ships the Debian bundle and nothing else. So far the only
hook is `[tls] truststore`, a pre-built JKS/PKCS12 mounted read-only and wired
into `JAVA_TOOL_OPTIONS`. That covers Maven and Gradle and nothing else: `curl`,
`git`, `pip`, `uv`, `npm`, `cargo`, the cloud CLIs, a future `podman pull` and
Claude Code itself (a Node process, which carries Mozilla's roots and ignores
the system bundle) all fail with a certificate error, and each one has its own
spelling for the fix.

ADR 0006 states that claude_here injects no certificate. That remains true:
claude_here intercepts nothing. The CA here is the user's, for the user's
network, and trusting it is the opposite of a MITM: it is what makes the
end-to-end TLS of ADR 0006 succeed at all behind a corporate proxy.

Users must not have to know the per-tool spelling. The sandbox exists so that
one configuration line replaces an afternoon of environment archaeology.

## Decision

`[tls] ca = "<path to a PEM file>"`, also `--ca <path>` on the command line.
The file may hold one certificate or a chain; one path, not a list — a company
bundle is one file.

Two halves, split along the purity line of `assemble()`:

* **`assemble()`** mounts the file read-only at
  `/usr/local/share/ca-certificates/claude_here.crt` — the directory
  `update-ca-certificates` scans — and sets, only when `ca` is given, the
  environment of the clients that do not read the system bundle:

  | Variable                             | Reader                               | Value             |
  |--------------------------------------|--------------------------------------|-------------------|
  | `NODE_EXTRA_CA_CERTS`                | Node, Claude Code, npm               | the mounted PEM   |
  | `REQUESTS_CA_BUNDLE`, `PIP_CERT`     | Python `requests`, `pip`, `az`       | the system bundle |
  | `UV_NATIVE_TLS=1`                    | `uv` (ships webpki roots by default) | —                 |
  | `AWS_CA_BUNDLE`                      | `aws`                                | the system bundle |
  | `CLOUDSDK_CORE_CUSTOM_CA_CERTS_FILE` | `gcloud`                             | the system bundle |

  "The system bundle" is `/etc/ssl/certs/ca-certificates.crt`, which after the
  root phase contains the company CA *and* the public roots, so no variable
  points at the company CA alone: a tool that is told only the company CA
  loses the public internet. `SSL_CERT_FILE` is deliberately not set for the
  same reason — OpenSSL clients read the system bundle anyway.

* **The root phase of the entrypoint**, before dropping to the sandbox user,
  runs `update-ca-certificates` when the mounted file exists, and, when the
  `jvm` toolchain is present, imports the PEM into the JDK's own `cacerts`
  with `keytool -importcert -noprompt`. Both write into the container's
  writable overlay; the image is unchanged and nothing of this is persisted.

Clients that read the system bundle need nothing further: `curl`, `git`,
`cargo`, Go programs (`gh`, `kubectl`, `helm`, `terraform`), Dart, `podman`.

`[tls] truststore` stays. It is the special case of a pre-built trust store
with content beyond one CA (client certificates, a store curated by an IT
department). When both are set, `JAVA_TOOL_OPTIONS` wins for the JVM, as it
does today; `ca` still serves every other client.

## Consequences

* The entrypoint grows a root-phase step; `images/entrypoint.sh` changes, so
  every image rebuilds once (changelog).
* The variable table is a maintenance list: a new toolchain whose client
  ignores the system bundle adds a row, and the `Toolchain` registry is the
  place to look for one that was forgotten. The test for `assemble()` pins
  the full set, so a row cannot vanish unnoticed.
* `keytool` import happens on every start (a second or two with the JVM
  toolchain, nothing without). Caching it would mean persisting part of the
  image's filesystem; not worth the state.
* The PEM is readable by every process in the container. It is a public
  certificate; the private key never leaves the proxy. No secret is mounted.
* Nothing here changes the three restriction axes. A proxy that *is* a MITM
  still sees only what the egress allowlist let through, and the capture of
  ADR 0003 records the same connections as before.
