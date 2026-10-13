#!/bin/sh
# Smoke test against a real ConfD/NSO NETCONF TCP port. Not part of CI.
#
#   NC_HOST (127.0.0.1)  NC_PORT (2023)  NC_BIN (target/debug/netconf-console)
#   NC_PROTO (tcp)  NC_PROTO=ssh NC_PORT=2022 runs the same checks over SSH
#   NC_PY_REF=/path/to/trunk/bin/netconf-console.py  also compares plain output with the Python script
#
# Needs a server with netconf TCP enabled, e.g. examples.confd/intro/1-2-3-start-query-model.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
HOST=${NC_HOST:-127.0.0.1}
PORT=${NC_PORT:-2023}
PROTO=${NC_PROTO:-tcp}
BIN=${NC_BIN:-$here/../../target/debug/netconf-console}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail() { echo "FAIL: $*" >&2; exit 1; }
rust() { "$BIN" --proto "$PROTO" --host "$HOST" --port "$PORT" "$@"; }

check() { # name, command output, expected substring
    case "$2" in *"$3"*) echo "ok   $1" ;; *) fail "$1: expected '$3' in: $2" ;; esac
}

hello=$(rust --hello -s plain)
check "hello capabilities" "$hello" "urn:ietf:params:netconf:base:1.0"
check "hello session-id" "$hello" "<session-id>"

check "get-config running" "$(rust --get-config -s plain)" "<data"
check "get-config candidate db is rejected cleanly or answered" "$(rust --get-config --db running -s plain)" "<rpc-reply"
check "get" "$(rust --get -s plain)" "<rpc-reply"
check "xpath filter" "$(rust --get-config -x /dhcp -s plain)" "<rpc-reply"
check "get with-defaults" "$(rust --get --with-defaults report-all -s plain)" "<rpc-reply"

printf '<dhcp xmlns="http://tail-f.com/ns/example/dhcpd"/>' > "$tmp/filter.xml"
check "subtree filter" "$(rust --get-config --subtree-filter "$tmp/filter.xml" -s plain)" "<rpc-reply"

check "rpc from stdin" "$(echo '<get-config><source><running/></source></get-config>' | rust --rpc - -s plain)" "<data"
check "rpc with attribute" "$(echo '<get/>' | rust --rpc - --rpc-attribute 'xmlns:x="urn:x"' -s plain)" "<rpc-reply"

check "default style is formatted xml" "$(rust --get-config)" '<?xml version="1.0"'
check "get-data" "$(rust --get-data -s plain)" "<rpc-reply"
check "get-schema prints the module text" "$(rust --get-schema ietf-netconf-monitoring)" "module ietf-netconf-monitoring"
check "validate" "$(rust --validate -s plain)" "<ok/>"
check "noaaa drops the aaa subtree" "$(rust --get -s noaaa | grep -c '<aaa')" "0"
if command -v xmllint > /dev/null; then
    [ "$(NETCONF_CONSOLE_XMLLINT=1 rust --get-config)" = "$(rust --get-config)" ] || fail "external xmllint differs from the built-in formatter"
    echo "ok   NETCONF_CONSOLE_XMLLINT=1 gives the same output"
fi

raw=$(rust --get-config -s raw)
check "raw keeps chunk framing" "$raw" "##"
check "bad rpc is answered with rpc-error" "$(echo '<no-such-op/>' | rust --rpc - -s plain)" "<rpc-error"

n=$(rust --get -s plain --iter 3 | grep -o '<rpc-reply' | wc -l | tr -d ' ')
[ "$n" -ge 1 ] || fail "iter: no reply"
echo "ok   iter 3 (plain prints the first reply: $n)"

if [ -n "${NC_PY_REF:-}" ]; then
    py() { python3 "$NC_PY_REF" --proto "$PROTO" --host "$HOST" --port "$PORT" "$@"; }
    # Operational data (session ids, memory, times) changes between calls, so digits are masked.
    mask() { sed 's/[0-9][0-9]*/N/g'; }
    for args in "--get-config" "--get" "--get-config -x /dhcp" "--get --with-defaults report-all"; do
        # shellcheck disable=SC2086
        [ "$(rust $args -s plain | mask)" = "$(py $args -s plain | mask)" ] || fail "differs from Python: $args"
        echo "ok   same as Python: $args"
    done
fi

echo "smoke test passed"
