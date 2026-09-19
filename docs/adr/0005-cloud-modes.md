# ADR 0005: Cloud modes, mirroring the git modes

Date: 2026-09-19
Status: accepted

## Context

Cluster and cloud CLIs (`kubectl`, `helm`, `terraform`, the vendor CLIs) are
part of everyday work, and the sandbox is meant to *enable* work under
selective restriction, not to withhold capabilities. But cloud access differs
from git in one decisive way: `ro` git mode is enforced by the kernel, while
nothing local can stop a network call. A read-only bind mount of
`~/.kube/config` protects the file, not the cluster — `kubectl delete
deployment` against production still succeeds.

## Decision

A third axis with the same shape as the git modes, default `none`:

* `none` — no credentials are mounted. The CLIs are present, so manifests,
  charts and IaC can be written, rendered and validated (`helm template`,
  `kustomize build`, `terraform validate`), but nothing can reach a live
  system.
* `ro` — credentials are mounted and `/usr/local/bin/{kubectl,helm,terraform,aws,gcloud,az}`
  are shims with a verb allowlist:
  * `kubectl`: `get`, `describe`, `logs`, `top`, `explain`, `api-resources`,
    `version`, `config view`. Denied: mutations, and in particular `exec` and
    `port-forward`, which are arbitrary code execution and a network bridge.
  * `helm`: `list`, `get`, `status`, `template`, `show`, `history`.
  * `terraform`: `init`, `validate`, `fmt`, `plan`, `show`, `output`,
    `providers`.
  * vendor CLIs: `describe*` / `list*` / `get*` verbs and `s3 ls`.
* `full` — no restriction.

The mode is written by the root phase to `/etc/claude_here/cloud_mode`, exactly
as `git_mode` is, so the sandbox user cannot flip it; it is stated in the
appended system prompt and in `CLAUDE_HERE_SESSION_INFO`. `claude_yolo` refuses
`full` unless `--i-know` is given.

Credential mounts need no flags of their own. `~/.kube` is mounted when the
`k8s` toolchain is enabled and the mode is not `none`; likewise `~/.aws` with
`aws`, `~/.config/gcloud` with `gcloud`, `~/.azure` with `azure`.

## Consequences

* `ro` is a guard rail, not a boundary — the same statement ADR 0002 makes
  about `commit` git mode. Real enforcement is RBAC/IAM on the user's side: a
  kubeconfig context bound to a read-only service account, a read-only role.
  This is documented, not hidden.
* `terraform plan` is allowed although it can take a state lock on a remote
  backend; read-only is not the same as side-effect-free.
* An MCP server with cloud credentials bypasses the shims entirely. See
  ADR 0007.
* The per-session network capture records every cloud API call, so the axis
  comes with an audit trail for free.
