#!/usr/bin/env python3
"""Differential test: runs command lines with the Python script and with ncrust against a
live NETCONF server and compares stdout, stderr and exit code.

Environment:
  NC_PY_REF  path of netconf-console.py (required)
  NC_BIN     the ncrust binary (default: ../../target/debug/ncrust)
  NC_HOST (127.0.0.1)  NC_PORT (2023)  NC_PROTO (tcp)   the server

Usage:
  run.py [--commands FILE] [--only TEXT] [--report FILE]
  run.py --self-test      Python against Python with a built-in server: must report no diff

Command file lines (see commands.txt):
  ARGS                         shell-style words, {fx} is the fixtures directory
  ARGS < FILE                  standard input from FILE
  @timeout=SECONDS ARGS        the command is killed after that long; what it printed is compared
  !diff KEY | ARGS             the outputs are expected to differ; KEY must be an entry of
                               plan/DEVIATIONS.md (a text in its table) or the run is a failure
"""
import argparse
import difflib
import os
import re
import shlex
import socket
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
FIXTURES = HERE / "fixtures"
DEVIATIONS = ROOT / "plan" / "DEVIATIONS.md"

# Values that legitimately differ between two runs against the same server.
NORMALIZERS = [
    (re.compile(r"<session-id>\d+</session-id>"), "<session-id>N</session-id>"),
    (re.compile(r"DEBUG: pid: \d+"), "DEBUG: pid: N"),
    (re.compile(r"(<up-time>|<uptime>)[^<]*"), r"\1T"),
    (re.compile(r"(<eventTime>)[^<]*"), r"\1T"),
]


def normalize(text):
    for pattern, replacement in NORMALIZERS:
        text = pattern.sub(replacement, text)
    return text


class Command:
    def __init__(self, line_number, text):
        self.line_number = line_number
        self.timeout = 60
        self.stdin = None
        self.expected_diff = None
        self.mask = False
        self.usage = False
        text = text.strip()
        if text.startswith("!diff"):
            key, _, text = text[len("!diff"):].partition("|")
            self.expected_diff = key.strip()
            text = text.strip()
        match = re.match(r"@timeout=(\d+)\s+(.*)", text)
        if match:
            self.timeout = int(match.group(1))
            text = match.group(2)
        while text.startswith("@"):
            directive, _, text = text.partition(" ")
            text = text.strip()
            if directive.startswith("@timeout="):
                self.timeout = int(directive.split("=", 1)[1])
            elif directive == "@mask":
                self.mask = True
            elif directive == "@usage":
                self.usage = True
            else:
                raise ValueError("line %d: unknown directive %s" % (line_number, directive))
        if " < " in text:
            text, _, self.stdin = text.partition(" < ")
            self.stdin = self.stdin.strip().replace("{fx}", str(FIXTURES))
        self.args = [a.replace("{fx}", str(FIXTURES)) for a in shlex.split(text)]

    def __str__(self):
        shown = " ".join(shlex.quote(a) for a in self.args).replace(str(FIXTURES), "{fx}")
        return shown + (" < " + self.stdin.replace(str(FIXTURES), "{fx}") if self.stdin else "")


def read_commands(path):
    commands = []
    for number, line in enumerate(Path(path).read_text().splitlines(), 1):
        if line.strip() and not line.lstrip().startswith("#"):
            commands.append(Command(number, line))
    return commands


def run_one(implementation, command, server):
    host, port, proto = server
    argv = implementation + ["--proto", proto, "--host", host, "--port", str(port)] + command.args
    stdin = open(command.stdin, "rb") if command.stdin else subprocess.DEVNULL
    env = dict(os.environ)
    env.pop("TAILF_NETCONF_FORCE_TCP", None)
    try:
        done = subprocess.run(
            argv, stdin=stdin, capture_output=True, timeout=command.timeout, cwd=FIXTURES, env=env
        )
        code, out, err = done.returncode, done.stdout, done.stderr
    except subprocess.TimeoutExpired as e:
        code, out, err = "timeout", e.stdout or b"", e.stderr or b""
    finally:
        if command.stdin:
            stdin.close()
    out, err = out.decode(errors="replace"), err.decode(errors="replace")
    if command.mask:
        # State data of the server (counters, ids, the login history) changes with every request.
        out = re.sub(r">\d+<", ">N<", out)
        out = re.sub(r"\n#\d+\n", "\n#N\n", out)
        out = re.sub(r"<login>.*?</login>\s*", "", out, flags=re.DOTALL)
        out = re.sub(r"<login-time>[^<]*</login-time>", "", out)
    if command.usage:
        # The usage text differs on purpose; the message is what counts.
        err = err.rsplit("error: ", 1)[-1] if "error: " in err else err
    return code, normalize(out), normalize(err)


def explain(left, right):
    parts = []
    names = ["exit code", "stdout", "stderr"]
    for name, a, b in zip(names, left, right):
        if a != b:
            if name == "exit code":
                parts.append("exit code: reference %s, candidate %s" % (a, b))
            else:
                changes = list(
                    difflib.unified_diff(
                        a.splitlines(), b.splitlines(), "reference", "candidate", n=0, lineterm=""
                    )
                )
                shown = "\n    ".join(line[:200] for line in changes[:14])
                parts.append("%s differs:\n    %s" % (name, shown))
    return "\n  ".join(parts)


def deviation_known(key):
    return key in DEVIATIONS.read_text()


def compare_all(reference, candidate, commands, server, only=None, out=sys.stdout, lines_only=None):
    same = expected = bad = 0
    lines = []
    for command in commands:
        if only and only not in str(command):
            continue
        if lines_only and command.line_number not in lines_only:
            continue
        began = time.time()
        ref = run_one(reference, command, server)
        cand = run_one(candidate, command, server)
        slow = time.time() - began
        if slow > 10:
            print("  (slow: %.0f s) %s" % (slow, command), file=sys.stderr)
        identical = ref == cand
        if command.expected_diff is not None:
            if identical:
                bad += 1
                lines.append("STALE     line %d: marked !diff but identical: %s" % (command.line_number, command))
            elif not deviation_known(command.expected_diff):
                bad += 1
                lines.append(
                    "UNKNOWN   line %d: '%s' is not in plan/DEVIATIONS.md: %s"
                    % (command.line_number, command.expected_diff, command)
                )
            else:
                expected += 1
                lines.append("expected  line %d [%s]: %s" % (command.line_number, command.expected_diff, command))
        elif identical:
            same += 1
            lines.append("same      line %d: %s" % (command.line_number, command))
        else:
            bad += 1
            lines.append("DIFFERENT line %d: %s\n  %s" % (command.line_number, command, explain(ref, cand)))
    for line in lines:
        print(line, file=out)
    print("\n%d same, %d expected differences, %d problems" % (same, expected, bad), file=out)
    return same, expected, bad, lines


# A tiny NETCONF 1.0 server for the self-test: answers every rpc with <ok/> or <data/>.
class FakeServer:
    def __init__(self):
        self.listener = socket.socket()
        self.listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.listener.bind(("127.0.0.1", 0))
        self.listener.listen(8)
        self.port = self.listener.getsockname()[1]
        threading.Thread(target=self.accept_loop, daemon=True).start()

    def accept_loop(self):
        while True:
            try:
                conn, _ = self.listener.accept()
            except OSError:
                return
            threading.Thread(target=self.serve, args=(conn,), daemon=True).start()

    def serve(self, conn):
        eom = b"]]>]]>"
        with conn:
            buf = b""
            while b"\n" not in buf:
                data = conn.recv(4096)
                if not data:
                    return
                buf += data
            buf = buf.split(b"\n", 1)[1]
            conn.sendall(
                b'<hello xmlns="urn:ietf:params:xml:ns:netconf:base:1.0"><capabilities>'
                b"<capability>urn:ietf:params:netconf:base:1.0</capability></capabilities>"
                b"<session-id>7</session-id></hello>" + eom
            )
            while True:
                while eom not in buf:
                    data = conn.recv(4096)
                    if not data:
                        return
                    buf += data
                message, _, buf = buf.partition(eom)
                text = message.decode(errors="replace")
                ident = re.search(r'message-id="([^"]*)"', text)
                ident = ident.group(1) if ident else "1"
                if "<hello" in text and "<rpc" not in text:
                    continue
                body = "<ok/>" if "close-session" in text or "edit-config" in text else "<data/>"
                reply = '<rpc-reply xmlns="urn:ietf:params:xml:ns:netconf:base:1.0" message-id="%s">%s</rpc-reply>' % (ident, body)
                conn.sendall(reply.encode() + eom)
                if "close-session" in text:
                    return


SELF_TEST = [
    "--hello",
    "--hello -s plain",
    "--get-config",
    "--get-config -s raw",
    "--get -x /a/b",
    "--edit-config {fx}/dhcp-edit.xml -s plain",
    "--rpc - < {fx}/rpc-get-config.xml",
    "--get-config --iter 2 -s all",
    "--get --with-defaults bogus",
    "--get-data -s noaaa",
]


def self_test(reference):
    server = FakeServer()
    address = ("127.0.0.1", server.port, "tcp")
    with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as f:
        f.write("\n".join(SELF_TEST) + "\n")
    commands = read_commands(f.name)
    os.unlink(f.name)
    print("== identical implementations must not differ")
    same, _, bad, _ = compare_all(reference, reference, commands, address)
    if bad:
        print("self-test FAILED: identical commands reported as different")
        return 1
    print("\n== a different implementation must be noticed")
    other = [sys.executable, "-c", "print('something else')"]
    _, _, bad, _ = compare_all(reference, other, commands[:2], address, out=open(os.devnull, "w"))
    if bad != 2:
        print("self-test FAILED: differences were not detected")
        return 1
    print("self-test passed (%d identical, differences detected)" % same)
    return 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--commands", default=str(HERE / "commands.txt"))
    parser.add_argument("--only")
    parser.add_argument("--lines", help="comma separated line numbers of the commands file")
    parser.add_argument("--report")
    parser.add_argument("--self-test", action="store_true")
    options = parser.parse_args()

    ref_path = os.environ.get("NC_PY_REF")
    if not ref_path:
        sys.exit("set NC_PY_REF to the path of netconf-console.py")
    reference = [sys.executable, ref_path]
    if options.self_test:
        sys.exit(self_test(reference))

    candidate = shlex.split(os.environ.get("NC_BIN", str(ROOT / "target" / "debug" / "ncrust")))
    server = (
        os.environ.get("NC_HOST", "127.0.0.1"),
        int(os.environ.get("NC_PORT", "2023")),
        os.environ.get("NC_PROTO", "tcp"),
    )
    commands = read_commands(options.commands)
    started = time.time()
    same, expected, bad, lines = compare_all(
        reference,
        candidate,
        commands,
        server,
        options.only,
        lines_only={int(n) for n in options.lines.split(",")} if options.lines else None,
    )
    print("(%.0f s)" % (time.time() - started))
    if options.report:
        with open(options.report, "w") as report:
            report.write("# Differential run\n\nServer %s:%s over %s; %d same, %d expected, %d problems.\n\n```\n%s\n```\n"
                         % (server[0], server[1], server[2], same, expected, bad, "\n".join(lines)))
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
