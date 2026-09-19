#!/usr/bin/env bash
# Summarize a session capture with tshark and hand pcap + summary to the host
# log directory. Runs as root at the end of the entrypoint.
# Usage: net-summary.sh <pcap> <out_dir> <session_id> <uid> <gid>
set -euo pipefail
pcap="$1"; out_dir="$2"; sid="$3"; uid="$4"; gid="$5"
[ -f "${pcap}" ] || exit 0
[ -d "${out_dir}" ] || exit 0

work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT
T=$'\t'
ts() { tshark -r "${pcap}" "$@" 2>/dev/null || true; }

# DNS: queries and answers (ip -> name)
ts -Y 'dns.flags.response==0' -T fields -e dns.qry.name | tr ',' '\n' | sed '/^$/d' | sort -u > "${work}/dns_q"
ts -Y 'dns.flags.response==1' -T fields -e dns.qry.name -e dns.a -e dns.aaaa -E separator="${T}" \
  | awk -F"${T}" '{ n=split($2, a, ","); for (i=1;i<=n;i++) if (a[i]!="") print a[i] "\t" $1;
                    n=split($3, b, ","); for (i=1;i<=n;i++) if (b[i]!="") print b[i] "\t" $1 }' \
  | sort -u > "${work}/dns_a"

# TLS SNI per tcp stream
ts -Y 'tls.handshake.type==1' -T fields -e tcp.stream -e tls.handshake.extensions_server_name -E separator="${T}" \
  | awk -F"${T}" '$2!=""' | sort -u > "${work}/sni"

# Plain HTTP requests
ts -Y 'http.request' -T fields -e http.host -e http.request.method -e http.request.uri -E separator="${T}" \
  | awk -F"${T}" '{ printf "%s http://%s%s\n", $2, $1, $3 }' | sort | uniq -c | sort -rn | head -200 \
  | awk '{ c=$1; $1=""; sub(/^ /,""); print $0 " (" c "x)" }' > "${work}/http"

# Per-stream accounting: direction from the SYN packet
ts -Y 'tcp' -T fields -e tcp.stream -e ip.src -e ipv6.src -e ip.dst -e ipv6.dst -e tcp.dstport -e frame.len -e tcp.flags.syn -e tcp.flags.ack -E separator="${T}" \
  | awk -F"${T}" -v snifile="${work}/sni" -v dnsfile="${work}/dns_a" '
    BEGIN {
      while ((getline l < snifile) > 0) { split(l, p, "\t"); sni[p[1]] = p[2] }
      while ((getline l < dnsfile) > 0) { split(l, p, "\t"); dns[p[1]] = p[2] }
    }
    {
      s=$1; src=($2!=""?$2:$3); dst=($4!=""?$4:$5); port=$6; len=$7+0; syn=$8; ack=$9
      if (syn ~ /^(1|True)$/ && ack ~ /^(0|False)$/ && !(s in local)) { local[s]=src; rip[s]=dst; rport[s]=port }
      if (!(s in local)) next
      if (src == local[s]) out[s]+=len; else inb[s]+=len
      pk++
    }
    END {
      for (s in local) {
        ip=rip[s]; name=(s in sni)?sni[s]:((ip in dns)?dns[ip]:ip)
        key=name "\t" ip "\t" rport[s]
        conns[key]++; bo[key]+=out[s]; bi[key]+=inb[s]; to+=out[s]; ti+=inb[s]
      }
      for (k in conns) printf "%s\t%d\t%d\t%d\n", k, conns[k], bo[k], bi[k]
      printf "TOTAL\t%d\t%d\t%d\n", pk, to, ti > "/dev/stderr"
    }' 2>"${work}/total" > "${work}/hosts"

read -r _ packets bytes_out bytes_in < "${work}/total" || { packets=0; bytes_out=0; bytes_in=0; }

jq -n \
  --arg sid "${sid}" \
  --argjson packets "${packets:-0}" --argjson out "${bytes_out:-0}" --argjson in "${bytes_in:-0}" \
  --rawfile dns "${work}/dns_q" --rawfile hosts "${work}/hosts" --rawfile http "${work}/http" '
  {
    session_id: $sid, packets: $packets, bytes_out: $out, bytes_in: $in,
    dns_queries: ($dns | split("\n") | map(select(length > 0))),
    hosts: ($hosts | split("\n") | map(select(length > 0)) | map(split("\t") |
      { host: .[0], ip: .[1], port: (.[2] | tonumber), connections: (.[3] | tonumber),
        bytes_out: (.[4] | tonumber), bytes_in: (.[5] | tonumber) })),
    http_requests: ($http | split("\n") | map(select(length > 0)))
  }' > "${out_dir}/${sid}.summary.json"

cp "${pcap}" "${out_dir}/${sid}.pcap"
chown "${uid}:${gid}" "${out_dir}/${sid}.pcap" "${out_dir}/${sid}.summary.json"
