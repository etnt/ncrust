#!/usr/bin/env python3
"""Compares time and peak memory of netconf-console.py and ncrust (P5.5).

  NC_PY_REF=/path/to/netconf-console.py tests/perf/bench.py [--reply-mib 100] [--elements 1000000]
                                                           [--iterations 10000] [--release]

Scenarios (each against a local server, output to /dev/null):
  big reply   get-config answered with a reply of --reply-mib MiB, styles plain and pretty
  big edit    --edit-config with a file of --elements elements
  many        --get with --iter N, style last (the replies are small)

Peak memory is the maximum resident set size reported by `/usr/bin/time -l` (macOS) or
`-v` (Linux). Results are printed as a table; nothing is asserted here.
"""
import argparse
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent


def timing_prefix():
    if sys.platform == "darwin":
        return ["/usr/bin/time", "-l"], re.compile(r"(\d+)\s+maximum resident set size"), 1
    return ["/usr/bin/time", "-v"], re.compile(r"Maximum resident set size \(kbytes\): (\d+)"), 1024


class Server:
    def __init__(self, reply_mib):
        self.process = subprocess.Popen(
            [sys.executable, str(HERE / "server.py"), "--reply-mib", str(reply_mib)],
            stdout=subprocess.PIPE,
            text=True,
        )
        self.port = int(self.process.stdout.readline())

    def stop(self):
        self.process.kill()
        self.process.wait()


def measure(command, port, args, timeout=1800):
    prefix, pattern, unit = timing_prefix()
    argv = prefix + command + ["--proto", "tcp", "--host", "127.0.0.1", "--port", str(port)] + args
    started = time.perf_counter()
    done = subprocess.run(argv, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True, timeout=timeout)
    elapsed = time.perf_counter() - started
    found = pattern.search(done.stderr)
    memory = int(found.group(1)) * unit / (1 << 20) if found else float("nan")
    return done.returncode, elapsed, memory


def make_edit_file(path, elements):
    with open(path, "w") as f:
        f.write('<config xmlns="urn:bench">')
        line = "<e><k>%d</k><v>abcdefghijklmnopqrstuvwxyz</v></e>\n"
        for i in range(elements):
            f.write(line % i)
        f.write("</config>")
    return os.path.getsize(path) / (1 << 20)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--reply-mib", type=int, default=100)
    parser.add_argument("--elements", type=int, default=1_000_000)
    parser.add_argument("--iterations", type=int, default=10_000)
    parser.add_argument("--release", action="store_true", help="use target/release")
    options = parser.parse_args()

    reference = [sys.executable, os.environ["NC_PY_REF"]]
    binary = ROOT / "target" / ("release" if options.release else "debug") / "netconf-console"
    implementations = [("python", reference), ("ncrust", [str(binary)])]

    with tempfile.TemporaryDirectory() as tmp:
        edit_file = Path(tmp) / "edit.xml"
        size = make_edit_file(edit_file, options.elements)
        scenarios = [
            ("get-config, %d MiB reply, plain" % options.reply_mib, options.reply_mib, ["--get-config", "-s", "plain"]),
            ("get-config, %d MiB reply, pretty" % options.reply_mib, options.reply_mib, ["--get-config"]),
            ("edit-config, %d elements (%.0f MiB)" % (options.elements, size), 0, ["--edit-config", str(edit_file), "-s", "plain"]),
            ("get, --iter %d, last" % options.iterations, 0, ["--get", "--iter", str(options.iterations), "-s", "last"]),
        ]
        print("%-44s %-8s %9s %10s" % ("scenario", "client", "seconds", "peak MiB"))
        for name, reply_mib, args in scenarios:
            for label, command in implementations:
                server = Server(reply_mib)
                try:
                    code, seconds, memory = measure(command, server.port, args)
                finally:
                    server.stop()
                note = "" if code == 0 else "  (exit code %d)" % code
                print("%-44s %-8s %9.1f %10.0f%s" % (name, label, seconds, memory, note), flush=True)


if __name__ == "__main__":
    main()
