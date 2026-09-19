# ADR 0001: Rust binary instead of shell functions

Date: 2026-09-19
Status: accepted

## Context

copilot_here ships a .NET AOT binary plus sourced shell functions. The
equivalent for Claude Code needs argument splitting (tool flags vs. forwarded
claude flags), layered TOML configuration, image hashing, and report tables
for the network log.

## Decision

One Rust crate, two binaries (`claude_here`, `claude_yolo`) sharing a library.
Docker is driven through the `docker` CLI via `std::process::Command`, not the
API socket, so TTY handling is inherited and a compatible CLI (podman) can be
substituted with `CLAUDE_HERE_DOCKER`.

## Consequences

* Argument assembly is pure and unit-tested (`run::assemble`, `docker::RunSpec`).
* No runtime dependency on Node/.NET/Python on the host.
* Fish completions are generated from the clap definition.
