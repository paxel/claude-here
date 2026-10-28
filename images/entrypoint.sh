#!/usr/bin/env bash
# Root phase of every claude_here session:
#   1. publish the git and cloud modes where the sandbox user cannot change them
#   2. apply the egress allowlist and start the capture as user `netlog`
#   3. sync the bundled skills and plugin, drop to the sandbox user, run
#   4. on exit, stop the capture, summarize and hand the files to the host
set -euo pipefail

: "${CH_USER:?}" "${CH_UID:?}" "${CH_GID:?}" "${CH_SESSION_ID:?}"
CH_GIT_MODE="${CH_GIT_MODE:-ro}"
CH_CLOUD_MODE="${CH_CLOUD_MODE:-none}"
CH_NET_CAPTURE="${CH_NET_CAPTURE:-1}"
CH_NET_MODE="${CH_NET_MODE:-full}"
CH_NET_ALLOW="${CH_NET_ALLOW:-}"
CH_PLUGIN_DIR="${CH_PLUGIN_DIR:-}"
CH_CONTAINERS="${CH_CONTAINERS:-0}"
CAP_DIR=/var/log/claude_here
OUT_DIR=/var/log/claude_here_out
HOME_DIR="/home/${CH_USER}"

# The image was built for exactly this uid/gid; refuse to run as anything else.
img_uid="$(id -u "${CH_USER}" 2>/dev/null || echo none)"
img_gid="$(id -g "${CH_USER}" 2>/dev/null || echo none)"
if [ "${img_uid}" != "${CH_UID}" ] || [ "${img_gid}" != "${CH_GID}" ]; then
  echo "claude_here: image user ${CH_USER} is ${img_uid}:${img_gid}, host is ${CH_UID}:${CH_GID}; rebuild with --rebuild" >&2
  exit 125
fi

mkdir -p /etc/claude_here
printf '%s\n' "${CH_GIT_MODE}" > /etc/claude_here/git_mode
printf '%s\n' "${CH_CLOUD_MODE}" > /etc/claude_here/cloud_mode
printf '%s\n' "${CH_NET_MODE}" > /etc/claude_here/net_mode
chmod 644 /etc/claude_here/git_mode /etc/claude_here/cloud_mode /etc/claude_here/net_mode

# Company CA (ADR 0009): extend the system bundle, and the JDK's own store
# when the jvm toolchain is present. Both land in the writable overlay only.
CA_CRT=/usr/local/share/ca-certificates/claude_here.crt
if [ -f "${CA_CRT}" ]; then
  update-ca-certificates >/dev/null 2>&1 ||
    echo "claude_here: warning: update-ca-certificates failed; the company CA is not trusted" >&2
  /usr/local/lib/claude_here/ca-jvm.sh "${CA_CRT}" >/dev/null 2>&1 ||
    echo "claude_here: warning: keytool import failed; the JVM does not trust the company CA" >&2
fi

# Egress allowlist before anything else runs, so nothing slips out first.
if [ "${CH_NET_MODE}" = "allowlist" ]; then
  /usr/local/lib/claude_here/net-allowlist.sh "${CH_NET_ALLOW}"
fi

TCPDUMP_PID=""
if [ "${CH_NET_CAPTURE}" = "1" ]; then
  mkdir -p "${CAP_DIR}"
  chown netlog:netlog "${CAP_DIR}"
  chmod 700 "${CAP_DIR}"
  # setsid: own session, so a Ctrl-C typed into Claude does not reach tcpdump
  setsid tcpdump -i any -s 512 -U --immediate-mode -Z netlog \
    -w "${CAP_DIR}/${CH_SESSION_ID}.pcap" >/dev/null 2>"${CAP_DIR}/tcpdump.err" &
  TCPDUMP_PID=$!
fi

# The bundled skills and language servers live in a read-only mount. Copy the
# skills into the persistent home (they must be writable-adjacent to be found)
# and register the marketplace once per plugin version, as the sandbox user.
if [ -n "${CH_PLUGIN_DIR}" ] && [ -d "${CH_PLUGIN_DIR}" ]; then
  want="$(cat "${CH_PLUGIN_DIR}/.hash" 2>/dev/null || echo none)"
  have="$(cat "${HOME_DIR}/.claude/.claude_here_plugin" 2>/dev/null || echo none)"
  run_as_user() {
    env HOME="${HOME_DIR}" USER="${CH_USER}" LOGNAME="${CH_USER}" \
      setpriv --reuid="${CH_UID}" --regid="${CH_GID}" --init-groups -- "$@"
  }
  if [ "${want}" != "${have}" ]; then
    run_as_user mkdir -p "${HOME_DIR}/.claude/skills"
    run_as_user rm -rf "${HOME_DIR}/.claude/skills/claude_here"
    run_as_user cp -r "${CH_PLUGIN_DIR}/skills" "${HOME_DIR}/.claude/skills/claude_here"
    # Registration is best effort: the skills above work without it, and a
    # failure here must never keep the session from starting.
    if run_as_user claude plugin marketplace add "${CH_PLUGIN_DIR}" >/dev/null 2>&1 ||
       run_as_user claude plugin marketplace update claude_here >/dev/null 2>&1; then
      run_as_user claude plugin install claude-here@claude_here -y >/dev/null 2>&1 ||
        echo "claude_here: note: could not enable the bundled plugin; language servers stay off" >&2
    else
      echo "claude_here: note: could not register the bundled marketplace; language servers stay off" >&2
    fi
    run_as_user sh -c "printf '%s\n' \"${want}\" > \"${HOME_DIR}/.claude/.claude_here_plugin\""
  fi
fi

# Containers inside the sandbox (ADR 0010): rootless podman as the sandbox
# user needs a runtime dir, a storage root it owns (the named volume arrives
# root-owned the first time) and the Docker API socket API clients expect.
# Native overlay in a user namespace exists since kernel 5.11; nothing older
# gets a silent vfs fallback.
PODMAN_PID=""
if [ "${CH_CONTAINERS}" = "1" ]; then
  kv="$(uname -r | cut -d. -f1-2)"
  if [ "${kv%%.*}" -lt 5 ] || { [ "${kv%%.*}" -eq 5 ] && [ "${kv#*.}" -lt 11 ]; }; then
    echo "claude_here: the docker toolchain needs kernel 5.11 or newer for rootless overlay; host kernel is $(uname -r)" >&2
    exit 125
  fi
  RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/${CH_UID}}"
  mkdir -p "${RUNTIME_DIR}/podman"
  chown "${CH_UID}:${CH_GID}" "${RUNTIME_DIR}" "${RUNTIME_DIR}/podman"
  chmod 700 "${RUNTIME_DIR}"
  storage="${HOME_DIR}/.local/share/containers/storage"
  if [ -d "${storage}" ] && [ "$(stat -c %u "${storage}")" != "${CH_UID}" ]; then
    chown "${CH_UID}:${CH_GID}" "${storage}"
  fi
  sock="${RUNTIME_DIR}/podman/podman.sock"
  rm -f "${sock}"
  env HOME="${HOME_DIR}" USER="${CH_USER}" LOGNAME="${CH_USER}" XDG_RUNTIME_DIR="${RUNTIME_DIR}" \
    setsid setpriv --reuid="${CH_UID}" --regid="${CH_GID}" --init-groups -- \
    podman system service --time=0 "unix://${sock}" >/dev/null 2>"${RUNTIME_DIR}/podman/service.err" &
  PODMAN_PID=$!
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    [ -S "${sock}" ] && break
    sleep 0.5
  done
  if [ ! -S "${sock}" ]; then
    echo "claude_here: warning: podman API socket did not appear; see ${RUNTIME_DIR}/podman/service.err" >&2
  fi
fi

CHILD=""
forward_term() { [ -n "${CHILD}" ] && kill -TERM "${CHILD}" 2>/dev/null || true; }
# INT reaches the child through the terminal's process group already.
trap ':' INT
trap forward_term TERM

env HOME="${HOME_DIR}" USER="${CH_USER}" LOGNAME="${CH_USER}" \
  setpriv --reuid="${CH_UID}" --regid="${CH_GID}" --init-groups -- "$@" <&0 &
CHILD=$!

rc=0
while :; do
  if wait "${CHILD}"; then
    rc=0
    break
  else
    rc=$?
    if kill -0 "${CHILD}" 2>/dev/null; then
      continue
    fi
    break
  fi
done

if [ -n "${PODMAN_PID}" ]; then
  kill -TERM "${PODMAN_PID}" 2>/dev/null || true
  wait "${PODMAN_PID}" 2>/dev/null || true
fi

if [ -n "${TCPDUMP_PID}" ]; then
  # let the last packets drain from the kernel buffer before stopping
  sleep 0.5
  kill -INT "${TCPDUMP_PID}" 2>/dev/null || true
  wait "${TCPDUMP_PID}" 2>/dev/null || true
  /usr/local/lib/claude_here/net-summary.sh \
    "${CAP_DIR}/${CH_SESSION_ID}.pcap" "${OUT_DIR}" "${CH_SESSION_ID}" "${CH_UID}" "${CH_GID}" || true
fi

exit "${rc}"
