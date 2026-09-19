#!/usr/bin/env bash
# Hand the session capture to the host log directory.
# Usage: net-summary.sh <pcap> <out_dir> <session_id> <uid> <gid>
set -euo pipefail
pcap="$1"; out_dir="$2"; sid="$3"; uid="$4"; gid="$5"
[ -f "${pcap}" ] || exit 0
[ -d "${out_dir}" ] || exit 0
cp "${pcap}" "${out_dir}/${sid}.pcap"
chown "${uid}:${gid}" "${out_dir}/${sid}.pcap"
