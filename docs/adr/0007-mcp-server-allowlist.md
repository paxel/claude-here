# ADR 0007: MCP servers are opt-in per project

Date: 2026-09-19
Status: accepted

## Context

Every restriction in this tool is enforced against a *process*: the git shim,
the cloud shims, the egress proxy. An MCP server sidesteps all of them, because
the capability arrives over a socket instead of a binary:

* a GitHub MCP server with a token can open pull requests and push while git
  mode is `ro`; the read-only `.git` mount is irrelevant because nothing local
  is written,
* a Kubernetes or AWS MCP server ignores the cloud shims of ADR 0005,
* an HTTP/SSE MCP server is a hole in the egress allowlist of ADR 0006 by
  construction — it *is* the allowlisted endpoint,
* `init` copied `mcpServers` out of the host `~/.claude.json`, so any
  credentials written inline in that file travelled into the container
  silently.

## Decision

Nothing is seeded. `mcpServers` is no longer copied from the host
configuration. A server reaches the container only when it is named in the
`mcp` list of the global or project configuration, the same shape `env` and
`mounts` already use.

The servers that were granted are visible where the other granted capabilities
are: the startup line and `CLAUDE_HERE_SESSION_INFO`.

## Consequences

* Users who relied on their host MCP servers inside the sandbox must list them
  once per project or globally. This is the point: an MCP server is a granted
  capability, not an inherited one.
* The README states plainly that an MCP server can grant reach that no shim in
  this tool can restrict.
