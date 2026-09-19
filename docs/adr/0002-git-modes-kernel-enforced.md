# ADR 0002: Git modes, `ro` enforced by the kernel

Date: 2026-09-19
Status: accepted

## Context

Prompt-level rules ("do not use git") are not reliable; an agent has switched
branches and pushed against explicit instructions. Enforcement must not depend
on the model's cooperation.

## Decision

Three modes, default `ro`:

* `ro` — every `.git` directory found under the working directory (plain
  repositories, nested repositories, and the git dirs pointed to by worktree /
  submodule `.git` files) is bind-mounted read-only on top of the read-write
  project mount. The kernel rejects any write. ssh agent and gh config are
  never mounted.
* `commit` — `.git` writable; `/usr/local/bin/git` is a shim that allows
  inspection and local commits and denies branch switching, history rewriting,
  tags, remote configuration, config writes and remote operations. The mode is
  read from a root-owned file (`/etc/claude_here/git_mode`), not from the
  environment, so the sandbox user cannot flip it.
* `full` — no restriction; `--ssh` (agent socket forwarded) and `--gh`
  (host `gh auth token` passed as `GH_TOKEN` through a per-run 0600 env-file;
  `~/.config/gh` is not mounted because keyring-backed logins keep no token
  there) become effective.

`claude_yolo` refuses `full` unless `--i-know` is given.

## Consequences

* Only `ro` is a security boundary. `commit` is a guard rail: `/usr/bin/git`
  or direct writes to `.git/` bypass it. This is accepted and documented.
* In `ro`, `git status` works; `git add`/`commit`/`checkout` fail with
  "Read-only file system" (verified).
* When the working directory is a subdirectory of a repository, the repository
  is invisible; a note is printed at start.
