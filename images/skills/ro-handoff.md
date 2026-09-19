---
name: claude-here-ro-handoff
description: How to finish work in a claude_here sandbox when git is read-only. Use when the session is in git mode 'ro' or 'commit' and work is ready to hand back to the user.
---

# Handing work back out of the sandbox

In git mode `ro` the `.git` directory is bind-mounted read-only by the kernel.
Staging, committing, checking out and pushing are impossible — not discouraged,
impossible. Do not spend turns discovering that again, and do not look for a way
around it. The working tree itself is writable, so the edits are real and the
user commits them on the host.

End such a session with a handover the user can act on without re-reading the
whole diff:

1. `git status --short` and `git diff --stat` still work. Use them; do not
   reconstruct the list from memory.
2. Group the changed files by intent, not by directory. One line per group
   saying what changed and why.
3. Name anything the user must do outside the sandbox: a migration to run, a
   dependency to install, a secret to set, a file that was deliberately left
   alone.
4. Propose a commit message in the repository's own style — check `git log` for
   whether it uses a subject line only, a prefix convention, or trailers.
5. State what you verified and what you did not. "Tests pass" and "not run"
   are both useful; "should work" is not.

In git mode `commit` you may stage and commit locally, but branching, rebasing,
resetting, stashing, tagging and every remote operation are denied by the `git`
wrapper. Commit on the branch that is already checked out, and let the user push.
