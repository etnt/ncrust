#!/bin/sh
# Key exchange interop against the installed OpenSSH server (P7.6): starts an sshd as the current
# user with a one-line NETCONF "server" as subsystem and prints what ncrust negotiates.
#
#   NC_BIN (target/debug/ncrust)   SSHD (/usr/sbin/sshd)
set -eu

here=$(cd "$(dirname "$0")" && pwd)
BIN=${NC_BIN:-$here/../../target/debug/ncrust}
SSHD=${SSHD:-/usr/sbin/sshd}
dir=$(mktemp -d)
trap 'kill "$pid" 2>/dev/null || true; rm -rf "$dir"' EXIT

ssh-keygen -q -t ed25519 -N '' -f "$dir/host"
ssh-keygen -q -t ed25519 -N '' -f "$dir/user"
cp "$dir/user.pub" "$dir/authorized_keys"

cat >"$dir/netconf" <<'EOF'
#!/usr/bin/env python3
import re, sys
end = b"]]>]]>"
hello = (b'<hello xmlns="urn:ietf:params:xml:ns:netconf:base:1.0"><capabilities>'
         b'<capability>urn:ietf:params:netconf:base:1.0</capability></capabilities>'
         b'<session-id>1</session-id></hello>' + end)
sys.stdout.buffer.write(hello); sys.stdout.buffer.flush()
buf = b""
while True:
    chunk = sys.stdin.buffer.read1(4096)
    if not chunk:
        break
    buf += chunk
    while end in buf:
        msg, buf = buf.split(end, 1)
        m = re.search(rb'message-id="([^"]*)"', msg)
        if m:
            reply = (b'<rpc-reply xmlns="urn:ietf:params:xml:ns:netconf:base:1.0" message-id="'
                     + m.group(1) + b'"><ok/></rpc-reply>' + end)
            sys.stdout.buffer.write(reply); sys.stdout.buffer.flush()
EOF
chmod +x "$dir/netconf"

port=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
cat >"$dir/sshd_config" <<EOF
Port $port
ListenAddress 127.0.0.1
HostKey $dir/host
AuthorizedKeysFile $dir/authorized_keys
PasswordAuthentication no
UsePAM no
StrictModes no
PidFile $dir/sshd.pid
Subsystem netconf $dir/netconf
EOF
"$SSHD" -f "$dir/sshd_config" -D -e 2>"$dir/sshd.log" &
pid=$!
sleep 1

echo "server: $("$SSHD" -V 2>&1 | head -1)"
run() {
    printf '%-40s ' "ncrust $*"
    "$BIN" --host 127.0.0.1 --port "$port" -u "$(id -un)" --privKeyFile "$dir/user" --debug --hello -s plain "$@" 2>&1 >/dev/null |
        grep -E 'SSH algorithms|SSH error|Unsupported' | head -1
}
run
run --ssh-kex curve25519-sha256
run --ssh-kex mlkem768x25519-sha256
