---
name: claude-here-net-triage
description: Inspect what earlier sessions of this project talked to. Use when a network dependency is unclear, a build needs an unknown host, or an egress allowlist has to be assembled.
---

# Reading your own network history

Every claude_here session leaves a summary. This sandbox can read the summaries
of earlier sessions **of this project only**, under
`~/.claude_here/net/`. One JSON file per session:

```
{"session_id":"...","packets":516,"bytes_out":639176,"bytes_in":194458,
 "dns_queries":["api.anthropic.com"],
 "hosts":[{"host":"api.anthropic.com","ip":"...","port":443,
           "connections":7,"bytes_out":639176,"bytes_in":194458}],
 "http_requests":["GET http://..."]}
```

Useful questions these answer:

* Which hosts does a build actually need? Collect `hosts[].host` across
  summaries — that is the raw material for `net_allow` under
  `--net allowlist`.
* Did a step reach something unexpected? An unfamiliar host with real byte
  counts is worth reporting to the user.
* Was traffic plaintext? Anything in `http_requests` went over HTTP, not HTTPS.

Limits, so you do not draw wrong conclusions:

* The packet captures themselves are **not** here. They contain request headers
  and would expose credentials from earlier sessions.
* Other projects' sessions are not here either.
* The current session's summary is written at exit, so it never appears during
  the session.
* A URL in `http_requests` can itself carry a token in its query string. Treat
  it as sensitive and do not repeat it verbatim unless it matters.
