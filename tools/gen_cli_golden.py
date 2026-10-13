#!/usr/bin/env python3
"""Regenerates tests/golden/cli_expected.json from the Python reference implementation.

For every command line in tests/golden/cli_cases.json:
  "parse":   Python's optparse is run (without running the script) and the parsed values or
             the error message are stored;
  "invalid": the real script is run (these command lines fail before any connection is made)
             and exit code, stdout and the last stderr line are stored.
Also stores every option of the parser (names, kind, default).

Usage: NC_PY_REF=/path/to/trunk/bin/netconf-console.py tools/gen_cli_golden.py
(defaults to ../bin/netconf-console.py, i.e. ncrust checked out inside trunk).
"""
import contextlib
import importlib.util
import io
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REF = Path(os.environ.get("NC_PY_REF", ROOT.parent / "bin" / "netconf-console.py"))
GOLDEN = ROOT / "tests" / "golden"


class Rejected(Exception):
    pass


def load_reference():
    spec = importlib.util.spec_from_file_location("netconf_console_ref", REF)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def new_parser(ref):
    parser = ref.build_options_parser()

    def error(message):
        raise Rejected(message)

    parser.error = error
    return parser


def option_kind(option):
    if option.action == "callback":
        return {"get_config_opt": "optional", "get_data_opt": "optional", "opt_xpath": "greedy"}[
            option.callback.__name__
        ]
    if option.action in ("store_true", "store_false"):
        return "flag"
    return "int" if option.type == "int" else "value"


def describe_options(parser):
    found = []
    for option in parser._get_all_options():
        if option.action == "help":
            continue
        found.append(
            {
                "dest": option.dest,
                "longs": [name[2:] for name in option._long_opts],
                "shorts": [name[1:] for name in option._short_opts],
                "kind": option_kind(option),
                "default": parser.defaults.get(option.dest),
            }
        )
    return found


def parse(ref, args):
    parser = new_parser(ref)
    sink = io.StringIO()
    try:
        with contextlib.redirect_stdout(sink):
            values, positional = parser.parse_args(list(args))
    except Rejected as e:
        return {"args": args, "error": str(e)}
    except SystemExit as e:
        return {"args": args, "help": True, "exit": e.code}
    return {"args": args, "values": vars(values), "positional": positional}


def run_script(args):
    env = dict(os.environ)
    env.pop("TAILF_NETCONF_FORCE_TCP", None)
    done = subprocess.run(
        [sys.executable, str(REF), *args],
        stdin=subprocess.DEVNULL,
        capture_output=True,
        text=True,
        timeout=30,
        env=env,
    )
    stderr_lines = [line for line in done.stderr.splitlines() if line.strip()]
    last = stderr_lines[-1] if stderr_lines else ""
    message = last.split("error: ", 1)[1] if "error: " in last else None
    return {
        "args": args,
        "exit": done.returncode,
        "stdout": done.stdout,
        "usage_message": message,
        "stderr": "" if message is not None else done.stderr,
    }


def main():
    cases = json.loads((GOLDEN / "cli_cases.json").read_text())
    ref = load_reference()
    expected = {
        "options": describe_options(new_parser(ref)),
        "parse": [parse(ref, args) for args in cases["parse"]],
        "invalid": [run_script(args) for args in cases["invalid"]],
    }
    (GOLDEN / "cli_expected.json").write_text(json.dumps(expected, indent=1, sort_keys=True) + "\n")
    print(
        "wrote %d options, %d parse cases, %d invalid cases"
        % (len(expected["options"]), len(expected["parse"]), len(expected["invalid"]))
    )


if __name__ == "__main__":
    main()
