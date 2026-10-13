#!/bin/bash
# Runs a list of command lines with ncrust and with the Python script against a live
# ConfD/NSO TCP NETCONF port and reports every difference in output or exit code.
# Digits are masked, because session ids, memory sizes and times change between runs.
#
#   NC_PY_REF=/path/to/trunk/bin/netconf-console.py   (required)
#   NC_HOST (127.0.0.1)  NC_PORT (2023)  NC_BIN (target/debug/netconf-console)
#   NC_PROTO (tcp)  NC_PROTO=ssh NC_PORT=2022 compares over SSH (Python then needs paramiko)
#
# Needs the dhcpd example model (examples.confd/intro/1-2-3-start-query-model) in the server;
# cases that change data are idempotent.
set -u

here=$(cd "$(dirname "$0")" && pwd)
: "${NC_PY_REF:?set NC_PY_REF to the path of netconf-console.py}"
HOST=${NC_HOST:-127.0.0.1}
PORT=${NC_PORT:-2023}
PROTO=${NC_PROTO:-tcp}
BIN=${NC_BIN:-$here/../../target/debug/netconf-console}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

mask() { sed 's/[0-9][0-9]*/N/g'; }
same=0
different=0

compare() { # "arguments" [stdin file]
    local args="$1" input="${2:-/dev/null}"
    # shellcheck disable=SC2086
    timeout 30 "$BIN" --proto "$PROTO" --host "$HOST" --port "$PORT" $args < "$input" 2> "$tmp/r.err" | mask > "$tmp/r.out"
    local rc_rust=${PIPESTATUS[0]}
    # shellcheck disable=SC2086
    timeout 30 python3 "$NC_PY_REF" --proto "$PROTO" --host "$HOST" --port "$PORT" $args < "$input" 2> "$tmp/p.err" | mask > "$tmp/p.out"
    local rc_python=${PIPESTATUS[0]}
    if cmp -s "$tmp/r.out" "$tmp/p.out" && [ "$rc_rust" = "$rc_python" ]; then
        same=$((same + 1))
        echo "same      rc=$rc_rust $(wc -c < "$tmp/r.out" | tr -d ' ') bytes: $args"
    else
        different=$((different + 1))
        echo "DIFFERENT rc rust=$rc_rust python=$rc_python: $args"
        diff "$tmp/r.out" "$tmp/p.out" | head -8
        echo "  rust stderr:   $(head -c 200 "$tmp/r.err")"
        echo "  python stderr: $(head -c 200 "$tmp/p.err")"
    fi
}

printf '<get-config><source><running/></source></get-config>' > "$tmp/rpc.xml"
printf '<dhcp xmlns="http://tail-f.com/ns/example/dhcpd"><defaultLeaseTime>PT2H</defaultLeaseTime></dhcp>' > "$tmp/edit.xml"
printf '%s%s%s' \
    '<hello xmlns="urn:ietf:params:xml:ns:netconf:base:1.0"><capabilities><capability>urn:ietf:params:netconf:base:1.0</capability></capabilities></hello>]]>]]>' \
    '<rpc xmlns="urn:ietf:params:xml:ns:netconf:base:1.0" message-id="1"><get-config><source><running/></source></get-config></rpc>]]>]]>' \
    '<rpc xmlns="urn:ietf:params:xml:ns:netconf:base:1.0" message-id="2"><close-session/></rpc>]]>]]>' > "$tmp/session.xml"

for style in "" "-s all" "-s last" "-s noaaa" "-s raw" "-s plain"; do
    compare "--hello $style"
done
for style in "" "-s noaaa" "-s plain" "-s raw"; do
    compare "--get-config $style"
    compare "--get $style"
    compare "--get-data $style"
done
compare "--get-config -s all --iter 2"
compare "--get -s last --iter 3"
compare "--get-config -x /dhcp"
compare "--get-config candidate"
compare "--get-config -v 1.0"
compare "--get-config -v 1.1 -s all"
compare "--get-data operational"
compare "--get-data --max-depth 1"
compare "--get-data --with-origin -x /dhcp"
compare "--get-data --config-filter true"
compare "--get-data --origin-filter intended"
compare "--get-schema ietf-netconf-monitoring"
compare "--get-schema no-such-module"
compare "--get-schema ietf-netconf-monitoring -s pretty"
compare "--validate"
compare "--discard-changes"
compare "--commit"
compare "--kill-session 424242"
compare "--copy-running-to-startup"
compare "--kill-subscription 1"
compare "--rpc -" "$tmp/rpc.xml"
compare "--rpc - -s plain" "$tmp/rpc.xml"
compare "--edit-config -" "$tmp/edit.xml"
compare "--edit-config - -s plain" "$tmp/edit.xml"
compare "--edit-data - -s plain" "$tmp/edit.xml"
compare "--copy-config - -s plain" "$tmp/edit.xml"
compare "$tmp/session.xml"
compare "$tmp/session.xml -s all"
compare "$tmp/session.xml -s raw"

echo "$same same, $different different"
[ "$different" -eq 0 ]
