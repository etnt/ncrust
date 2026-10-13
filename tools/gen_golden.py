#!/usr/bin/env python3
"""Regenerates tests/golden/{hello,requests} from the Python reference implementation.

For every case in tests/golden/cases.json the real netconf-console.py is run against a small
fake NETCONF server (TCP, as `--proto tcp`) that records the requests it receives.

Usage: NC_PY_REF=/path/to/trunk/bin/netconf-console.py tools/gen_golden.py
(defaults to ../bin/netconf-console.py, i.e. ncrust checked out inside trunk).
"""
import importlib.util
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REF = Path(os.environ.get("NC_PY_REF", ROOT.parent / "bin" / "netconf-console.py"))
GOLDEN = ROOT / "tests" / "golden"

NC_NS = "urn:ietf:params:xml:ns:netconf:base:1.0"
SERVER_HELLO = (
    '<hello xmlns="%s"><capabilities>'
    "<capability>urn:ietf:params:netconf:base:1.0</capability>"
    "<capability>urn:ietf:params:netconf:base:1.1</capability>"
    "</capabilities><session-id>1</session-id></hello>" % NC_NS
)


def read_until(conn, buf, marker):
    while marker not in buf:
        data = conn.recv(65536)
        if not data:
            return None, buf
        buf += data
    head, _, rest = buf.partition(marker)
    return head, rest


def read_chunked(conn, buf):
    """One chunked-framing message; returns (message or None, remaining buffer)."""
    message = b""
    while True:
        head, rest = read_until(conn, buf, b"\n")
        if head is None:
            return None, buf
        # head is b"" for the leading LF of "\n#N\n"; read the header line again
        header, rest = read_until(conn, rest, b"\n")
        if header is None:
            return None, rest
        if header == b"##":
            return message, rest
        size = int(header[1:])
        while len(rest) < size:
            data = conn.recv(65536)
            if not data:
                return None, rest
            rest += data
        message += rest[:size]
        buf = rest[size:]


def frame(message):
    data = message.encode()
    return b"\n#%d\n%s\n##\n" % (len(data), data)


def serve(listener, case, received):
    conn, _ = listener.accept()
    with conn:
        _, buf = read_until(conn, b"", b"\n")  # IPC preamble line
        conn.sendall(SERVER_HELLO.encode() + b"]]>]]>")
        _, buf = read_until(conn, buf, b"]]>]]>")  # client hello
        if _ is None:
            return
        count = 0
        while True:
            message, buf = read_chunked(conn, buf)
            if message is None:
                return
            text = message.decode()
            msg_id = re.search(r'message-id="([^"]*)"', text).group(1)
            reply = '<rpc-reply xmlns="%s" message-id="%s"><ok/></rpc-reply>' % (NC_NS, msg_id)
            if "<close-session/>" in text:
                conn.sendall(frame(reply))
                return
            received.append(text)
            count += 1
            conn.sendall(frame(reply))
            if case.get("close_after_reply"):
                return


def run_case(case, tmp):
    files = {}
    for name, content in case.get("files", {}).items():
        path = Path(tmp) / name
        path.write_text(content)
        files[name] = str(path)
    args = [re.sub(r"\{f:(\w+)\}", lambda m: files[m.group(1)], a) for a in case["args"]]
    if "-s" not in args and "--outputStyle" not in args:
        args += ["-s", "raw"]  # raw needs no xmllint

    listener = socket.socket()
    listener.bind(("127.0.0.1", 0))
    listener.listen(1)
    received = []
    thread = threading.Thread(target=serve, args=(listener, case, received))
    thread.start()
    port = listener.getsockname()[1]
    proc = subprocess.run(
        [sys.executable, str(REF), "--proto", "tcp", "--host", "127.0.0.1", "--port", str(port)] + args,
        capture_output=True, timeout=30,
    )
    thread.join(timeout=10)
    listener.close()
    if not received:
        sys.exit("%s: no request received; stderr: %s" % (case["name"], proc.stderr.decode()))
    return received


def main():
    spec = importlib.util.spec_from_file_location("netconf_console_ref", REF)
    ref = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(ref)

    cases = json.loads((GOLDEN / "cases.json").read_text())
    for sub in ("hello", "requests"):
        shutil.rmtree(GOLDEN / sub, ignore_errors=True)
        (GOLDEN / sub).mkdir()

    for case in cases["hello"]:
        (GOLDEN / "hello" / (case["name"] + ".xml")).write_text(ref.hello_msg(case["versions"]))

    with tempfile.TemporaryDirectory() as tmp:
        for case in cases["cases"]:
            case_dir = GOLDEN / "requests" / case["name"]
            case_dir.mkdir()
            for k, message in enumerate(run_case(case, tmp), start=1):
                (case_dir / ("%d.xml" % k)).write_text(message)

    print("generated", len(cases["hello"]), "hello and", len(cases["cases"]), "request cases", file=sys.stderr)


main()
