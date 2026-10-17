# Behind a company CA, a proxy or a private registry

Inside a company network TLS is usually terminated by a proxy that presents a
certificate from the company's own CA. Every client in the container then fails
with "unknown issuer" until it trusts that CA. One configuration line fixes it
for all of them (ADR 0009).

## Trust the company CA

Export the CA as PEM (one certificate or the whole chain) and point the
configuration at it:

```toml
# ~/.config/claude_here/config.toml
[tls]
ca = "~/company/ca.pem"
```

Or for one run: `claude_here --ca ~/company/ca.pem`.

At session start the PEM is added to the system bundle, and the clients that
keep their own root store are pointed at it:

| Client                                     | How it learns about the CA                  |
|--------------------------------------------|---------------------------------------------|
| `curl`, `git`, `cargo`, Go, Dart, `podman` | System bundle, nothing extra                |
| Node, Claude Code itself                   | `NODE_EXTRA_CA_CERTS`                       |
| `pip`, `requests`, `az`                    | `REQUESTS_CA_BUNDLE`, `PIP_CERT`            |
| `uv`                                       | `UV_NATIVE_TLS`                             |
| `aws`                                      | `AWS_CA_BUNDLE`                             |
| `gcloud`                                   | `CLOUDSDK_CORE_CUSTOM_CA_CERTS_FILE`        |
| JDK (Maven, Gradle, kotlinc)               | `keytool` import into the image's `cacerts` |

Nothing is intercepted and no certificate is generated: the CA is yours, for
your network.

## A JVM trust store with more in it

When the company hands out a ready-made JKS or PKCS12 with client certificates
or several CAs, mount it and name it instead of, or in addition to, `ca`:

```toml
[[mounts]]
path = "~/company/certs"
mode = "ro"

[tls]
ca = "~/company/ca.pem"
truststore = "~/company/certs/cacerts"
```

`truststore` is wired into `JAVA_TOOL_OPTIONS`; when both are set it wins for
the JVM, and `ca` still serves every other client.

## Proxy variables

Host environment variables reach the container only when named. For a proxy
that is the usual trio:

```toml
env = ["http_proxy", "https_proxy", "no_proxy"]
```

Clients that honour the variables (`curl`, `pip`, `npm`, Node) go through the
proxy; Maven and Gradle read their own settings files, which live in the
mounted `~/.m2` and `~/.gradle` caches.

## Private registries

The caches of a toolchain are mounted from the host, so a `~/.m2/settings.xml`
or `~/.npmrc` that points at the company mirror works inside without changes.
To keep a file with credentials out of the container:

```toml
[caches]
m2_exclude = ["settings.xml"]
```

Under `--net allowlist` the registry host has to be allowed; the toolchains
only allow their public defaults:

```toml
net_mode = "allowlist"
net_allow = ["nexus.example.com", "registry.example.com"]
```

The allowlist is a packet filter on host names, so it also restricts clients
that ignore `http_proxy`. Build the list empirically: one run with `--net
full`, then `claude_here net top --project` shows what was contacted.

## Checking that it works

```sh
claude_here --jvm --ca ~/company/ca.pem -p "run curl -sI https://nexus.example.com and report the status line"
```

A `200` or `401` means TLS is fine; `SSL certificate problem` means the PEM is
not the CA that signs the proxy's certificate.
