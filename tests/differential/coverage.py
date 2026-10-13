#!/usr/bin/env python3
"""Lists the options of netconf-console.py that no command of the differential corpus uses.

Usage: NC_PY_REF=/path/to/netconf-console.py tests/differential/coverage.py [COMMANDS...]
(default: commands.txt and commands-ssh.txt). Exit status 1 if an option is unused.
"""
import importlib.util
import os
import shlex
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
commands = [Path(a) for a in sys.argv[1:]] or [HERE / "commands.txt", HERE / "commands-ssh.txt"]
# Given to every command by run.py.
HARNESS = {"--host", "--port", "--proto"}
# Needs a server that calls home; tested against an SSH server fixture instead.
ELSEWHERE = {
    "--listen": "tests/ssh_e2e.rs call_home_waits_for_the_server_to_connect",
    "--listen-address": "tests/ssh_e2e.rs call_home_waits_for_the_server_to_connect",
    "--listen-port": "tests/ssh_e2e.rs call_home_waits_for_the_server_to_connect",
}

ref = os.environ.get("NC_PY_REF")
if not ref:
    sys.exit("set NC_PY_REF to the path of netconf-console.py")

spec = importlib.util.spec_from_file_location("ref", ref)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
parser = module.build_options_parser()

used = set()
lines = [line for path in commands for line in path.read_text().splitlines()]
for line in lines:
    line = line.strip()
    if not line or line.startswith("#"):
        continue
    line = line.split("|", 1)[1] if line.startswith("!diff") else line
    line = line.split(" < ", 1)[0]
    words = shlex.split(line)
    index = 0
    while index < len(words):
        word = words[index]
        index += 1
        if word == "--":
            break
        option = None
        if word.startswith("--"):
            try:
                option = parser._long_opt[parser._match_long_opt(word.split("=", 1)[0])]
            except Exception:
                pass
            if option is not None:
                used.add(option)
        elif word.startswith("-") and len(word) > 1:
            for char in word[1:]:
                option = parser._short_opt.get("-" + char)
                if option is None:
                    break
                used.add(option)
                if option.takes_value():
                    break

unused = []
total = 0
for option in parser._get_all_options():
    if option.action == "help":
        continue
    total += 1
    names = option._long_opts
    if option in used or any(n in HARNESS for n in names):
        continue
    if any(n in ELSEWHERE for n in names):
        print("covered elsewhere:", names[0], "->", ELSEWHERE[names[0]])
        continue
    unused.append(option)
    print("not used:", "/".join(option._short_opts + option._long_opts))
print("%d of %d options are used by the differential corpus" % (total - len(unused), total))
sys.exit(1 if unused else 0)
