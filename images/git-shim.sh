#!/usr/bin/env bash
# claude_here git shim (installed as /usr/local/bin/git).
#
# ro   : passthrough — the kernel enforces the read-only .git mount.
# full : passthrough.
# commit: allow inspection and local commits only; deny anything that
#         switches branches, rewrites history or talks to remotes.
#         This is a speed bump, not a security boundary (documented).
set -u
REAL=/usr/bin/git
mode="$(cat /etc/claude_here/git_mode 2>/dev/null || echo ro)"
if [ "${mode}" != "commit" ]; then
  exec "${REAL}" "$@"
fi

args=("$@")
n=${#args[@]}
i=0
sub=""
while [ "${i}" -lt "${n}" ]; do
  a="${args[$i]}"
  case "${a}" in
    -C|-c|--git-dir|--work-tree|--namespace|--exec-path|--super-prefix|--config-env)
      i=$((i + 2)); continue ;;
    -*)
      i=$((i + 1)); continue ;;
    *)
      sub="${a}"; break ;;
  esac
done
rest=("${args[@]:$((i + 1))}")

deny() {
  echo "git ${sub} ${*}: denied by claude_here (git mode: commit). Allowed: status diff log show blame add rm mv commit restore fetch rev-parse ls-files ... and read-only branch/tag/remote/config. Start with --git full to lift." >&2
  exit 125
}

has_write_flag() {
  # any argument that is not a read-only listing flag
  for r in "$@"; do
    case "${r}" in
      --list|-l|-a|-r|-v|-vv|--all|--remotes|--show-current|--contains|--merged|--no-merged|--points-at|--sort=*|--format=*|--column|--no-column|--color|--no-color|-n|--) ;;
      -*) return 0 ;;
      *) ;;
    esac
  done
  return 1
}

case "${sub}" in
  ""|status|diff|log|show|blame|rev-parse|ls-files|ls-tree|ls-remote|cat-file|describe|shortlog|grep|reflog|name-rev|merge-base|rev-list|count-objects|var|version|help|check-ignore|check-attr|diff-tree|diff-index|diff-files|show-ref|for-each-ref|whatchanged|cherry|fsck|verify-commit|verify-tag|hash-object|mailinfo|stripspace|interpret-trailers|range-diff|format-patch)
    exec "${REAL}" "$@" ;;
  add|rm|mv|commit|restore|fetch|apply|stage)
    exec "${REAL}" "$@" ;;
  branch)
    if [ "${#rest[@]}" -eq 0 ] || ! has_write_flag "${rest[@]}"; then
      # creating a branch is `git branch <name>`: a bare positional is a write
      for r in "${rest[@]:-}"; do
        case "${r}" in -*|"") ;; *) deny "${rest[@]}" ;; esac
      done
      exec "${REAL}" "$@"
    fi
    deny "${rest[@]}" ;;
  tag)
    if [ "${#rest[@]}" -eq 0 ]; then exec "${REAL}" "$@"; fi
    for r in "${rest[@]}"; do
      case "${r}" in -l|--list|-n|-n*|--contains|--points-at|--sort=*|--format=*|--column|--no-column|--merged|--no-merged) ;; *) deny "${rest[@]}" ;; esac
    done
    exec "${REAL}" "$@" ;;
  remote)
    case "${rest[0]:-}" in ""|-v|--verbose|show|get-url) exec "${REAL}" "$@" ;; *) deny "${rest[@]}" ;; esac ;;
  config)
    for r in "${rest[@]:-}"; do
      case "${r}" in --get|--get-all|--get-regexp|--list|-l|--show-origin|--show-scope|--global|--local|--system|--worktree|-z|--null|--type=*|--bool|--int|--path|--name-only|"") ;;
        -*) deny "${rest[@]}" ;;
        *) ;;
      esac
    done
    # `git config key value` (two positionals) is a write
    pos=0
    for r in "${rest[@]:-}"; do case "${r}" in -*|"") ;; *) pos=$((pos + 1)) ;; esac; done
    if [ "${pos}" -ge 2 ]; then deny "${rest[@]}"; fi
    exec "${REAL}" "$@" ;;
  *)
    deny "${rest[@]}" ;;
esac
