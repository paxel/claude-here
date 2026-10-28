#!/bin/sh
# Import every certificate of a PEM file into the JDK's own trust store
# (ADR 0009). `keytool -importcert` reads only the first certificate of a
# file, so the file is split and each certificate gets an alias of its own:
# claude_here-1, claude_here-2, ... An alias that exists already (from the jvm
# layer build, or from an earlier session) is replaced.
#   ca-jvm.sh <pem> [keytool]
# Exits 0 without doing anything when there is no keytool.
set -eu
pem="$1"
keytool="${2:-/opt/graalvm/bin/keytool}"
[ -x "${keytool}" ] || exit 0
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT
awk -v dir="${tmp}" '
  /-----BEGIN CERTIFICATE-----/ { n++; out = dir "/" n ".pem" }
  out != "" { print > out }
  /-----END CERTIFICATE-----/ { close(out); out = "" }
' "${pem}"
n=0
for f in "${tmp}"/*.pem; do
  [ -f "${f}" ] || continue
  n=$((n + 1))
  alias="claude_here-${n}"
  "${keytool}" -delete -alias "${alias}" -cacerts -storepass changeit >/dev/null 2>&1 || true
  "${keytool}" -importcert -noprompt -cacerts -storepass changeit \
    -alias "${alias}" -file "${f}" >/dev/null
done
[ "${n}" -gt 0 ] || { echo "ca-jvm.sh: no certificate in ${pem}" >&2; exit 1; }
