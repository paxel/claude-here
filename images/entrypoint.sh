#!/usr/bin/env bash
# Root phase of every claude_here session:
#   1. publish the git mode where the sandbox user cannot change it
#   2. start the packet capture as user `netlog`
#   3. drop to the sandbox user and run the command
#   4. on exit, stop the capture, summarize and hand the files to the host
set -euo pipefail

: "${CH_USER:?}" "${CH_UID:?}" "${CH_GID:?}" "${CH_SESSION_ID:?}"
CH_GIT_MODE="${CH_GIT_MODE:-ro}"
CH_NET_CAPTURE="${CH_NET_CAPTURE:-1}"
CAP_DIR=/var/log/claude_here
OUT_DIR=/var/log/claude_here_out
HOME_DIR="/home/${CH_USER}"

mkdir -p /etc/claude_here
printf '%s\n' "${CH_GIT_MODE}" > /etc/claude_here/git_mode
chmod 644 /etc/claude_here/git_mode

TCPDUMP_PID=""
if [ "${CH_NET_CAPTURE}" = "1" ]; then
  mkdir -p "${CAP_DIR}"
  chown netlog:netlog "${CAP_DIR}"
  chmod 700 "${CAP_DIR}"
  tcpdump -i any -s 512 -U --immediate-mode -Z netlog \
    -w "${CAP_DIR}/${CH_SESSION_ID}.pcap" >/dev/null 2>"${CAP_DIR}/tcpdump.err" &
  TCPDUMP_PID=$!
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

if [ -n "${TCPDUMP_PID}" ]; then
  # let the last packets drain from the kernel buffer before stopping
  sleep 0.5
  kill -INT "${TCPDUMP_PID}" 2>/dev/null || true
  wait "${TCPDUMP_PID}" 2>/dev/null || true
  /usr/local/lib/claude_here/net-summary.sh \
    "${CAP_DIR}/${CH_SESSION_ID}.pcap" "${OUT_DIR}" "${CH_SESSION_ID}" "${CH_UID}" "${CH_GID}" || true
fi

exit "${rc}"
