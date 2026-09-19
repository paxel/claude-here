#!/usr/bin/env bash
# claude_here cloud shim. Installed as symlinks in
# /usr/local/lib/claude_here/shims, which is first on PATH, for kubectl, helm,
# terraform, aws, gcloud and az.
#
# none : the tool runs, but no credentials are mounted, so there is nothing
#        live to reach. Passthrough.
# ro   : allow read verbs only. `kubectl exec` and `port-forward` are denied
#        because they are arbitrary code execution and a network bridge.
# full : passthrough.
#
# Like the git shim in `commit` mode this is a guard rail, not a boundary: the
# real binary can be called by its absolute path. Actual enforcement is RBAC or
# IAM on the user's side (ADR 0005).
set -u
SHIM_DIR=/usr/local/lib/claude_here/shims
name="$(basename "$0")"
mode="$(cat /etc/claude_here/cloud_mode 2>/dev/null || echo none)"

# The real binary is whatever comes next on PATH once the shim directory is out.
clean_path="$(printf '%s' "${PATH}" | tr ':' '\n' | grep -v -x "${SHIM_DIR}" | paste -sd: -)"
REAL="$(PATH="${clean_path}" command -v "${name}" 2>/dev/null || true)"
if [ -z "${REAL}" ]; then
  echo "claude_here: ${name} is not installed in this image; enable its toolchain" >&2
  exit 127
fi

if [ "${mode}" != "ro" ]; then
  exec "${REAL}" "$@"
fi

# First non-flag argument is the verb.
verb=""
for a in "$@"; do
  case "${a}" in
    -*) ;;
    *) verb="${a}"; break ;;
  esac
done

deny() {
  echo "${name} ${verb}: denied by claude_here (cloud mode: ro). ${1}" \
       "Start with --cloud full to lift." >&2
  exit 125
}

allowed_kubectl="get describe logs top explain api-resources api-versions version cluster-info config diff auth events wait"
allowed_helm="list get status template show history version env repo search dependency lint"
allowed_terraform="init validate fmt plan show output providers version graph state"
allowed_vendor_prefix="describe list get help version"

in_list() {
  needle="$1"
  shift
  for w in $1; do
    [ "${needle}" = "${w}" ] && return 0
  done
  return 1
}

case "${name}" in
  kubectl)
    case "${verb}" in
      exec|port-forward|attach|cp|proxy|debug)
        deny "Arbitrary execution inside a pod is never allowed in this mode." ;;
    esac
    if [ -z "${verb}" ] || in_list "${verb}" "${allowed_kubectl}"; then
      # `kubectl config` may only read.
      if [ "${verb}" = "config" ]; then
        for a in "$@"; do
          case "${a}" in
            view|get-contexts|current-context|get-clusters|get-users|--*|kubectl|config) ;;
            *) deny "Only read subcommands of kubectl config are allowed." ;;
          esac
        done
      fi
      exec "${REAL}" "$@"
    fi
    deny "Allowed: ${allowed_kubectl}." ;;
  helm)
    if [ -z "${verb}" ] || in_list "${verb}" "${allowed_helm}"; then
      exec "${REAL}" "$@"
    fi
    deny "Allowed: ${allowed_helm}." ;;
  terraform)
    if [ "${verb}" = "state" ]; then
      for a in "$@"; do
        case "${a}" in
          list|show|pull|terraform|state|-*) ;;
          *) deny "Only read subcommands of terraform state are allowed." ;;
        esac
      done
    fi
    if [ -z "${verb}" ] || in_list "${verb}" "${allowed_terraform}"; then
      exec "${REAL}" "$@"
    fi
    deny "Allowed: ${allowed_terraform}." ;;
  aws|gcloud|az)
    # Vendor CLIs are verb-last (`aws s3 ls`, `gcloud compute instances list`):
    # take the final non-flag word and accept read verbs and read prefixes.
    last=""
    for a in "$@"; do
      case "${a}" in -*) ;; *) last="${a}" ;; esac
    done
    case "${last}" in
      ls|list|describe|get|show|version|help|status|history) exec "${REAL}" "$@" ;;
      describe-*|list-*|get-*|show-*) exec "${REAL}" "$@" ;;
    esac
    if in_list "${verb}" "${allowed_vendor_prefix}"; then
      exec "${REAL}" "$@"
    fi
    deny "Allowed: commands ending in ls/list/describe/get/show." ;;
  *)
    exec "${REAL}" "$@" ;;
esac
