# Documentation

The [README](../README.md) describes every flag, mode and configuration key.
The guides here walk through one situation each.

## Guides

| Guide                                                          | For                                                                    |
|----------------------------------------------------------------|------------------------------------------------------------------------|
| [Running a testcontainers suite](testcontainers.md)            | Test suites that start databases or brokers as containers (`--docker`) |
| [Company CA, proxy, private registry](company-ca-and-proxy.md) | Corporate networks: `[tls] ca`, proxy variables, `net_allow`           |
| [Windows: WSL2](wsl.md)                                        | Installing and running inside a WSL distro                             |

## Example configurations

Ready-to-copy `config.toml` files in [`examples/`](../examples/): a minimal
project file, a corporate setup and a multi-toolchain project. Each one
documents the keys it uses; `claude_here config show` prints the merged result.

## Design decisions

The reasoning behind the locked decisions is in [`adr/`](adr/), one record per
decision: why a Rust binary, how git modes are enforced, the network capture,
the image chain, cloud modes, the egress allowlist, MCP grants, bundled skills,
the company CA mechanism and containers inside the sandbox.
