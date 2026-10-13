#!/usr/bin/env python3
"""Lists how netconf-console is used in the trunk tree (lux files, shell scripts, Makefiles).

Every invocation is parsed with the real Python option parser and reduced to its "shape":
the set of options used, whatever their values. The output (one line per shape, most
frequent first) is the raw material for tests/differential/commands.txt.

Usage: tools/extract_netconf_console_usage.py [TRUNK_DIR] > usage.tsv
Columns: count, shape, example arguments, example file.
"""
import contextlib
import importlib.util
import io
import os
import re
import shlex
import sys
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TRUNK = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT.parent
REF = Path(os.environ.get("NC_PY_REF", TRUNK / "bin" / "netconf-console.py"))

SUFFIXES = (".lux", ".luxinc", ".sh", ".mk", ".py", ".md", ".txt")
SKIP_DIRS = {"ncrust", "otp", ".git", "node_modules"}
COMMAND = re.compile(r"(?:^|[\s!$({\"'`/])netconf-console(?:-tcp|\.py)?(?=\s)(.*)$")
# Connection details differ between the places a test runs; the harness supplies its own.
CONNECTION = {"host", "port", "username", "password", "proto"}


def reference_parser():
    spec = importlib.util.spec_from_file_location("ref", REF)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    parser = module.build_options_parser()

    def error(message):
        raise ValueError(message)

    parser.error = error
    return parser


def files(top):
    for path, dirs, names in os.walk(top):
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS and not d.startswith("build-")]
        for name in names:
            if name.endswith(SUFFIXES) or name.startswith("Makefile"):
                yield Path(path) / name


def tokens(text):
    lexer = shlex.shlex(text, posix=True, punctuation_chars=True)
    lexer.whitespace_split = True
    out = []
    for token in lexer:
        if all(c in "();<>|&" for c in token):
            break  # a pipe, redirection or the end of the command
        out.append(token)
    return out


def shape(parser, args):
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            values, positional = parser.parse_args(args)
    except (ValueError, SystemExit):
        return None
    used = []
    for dest, value in sorted(vars(values).items()):
        if dest in CONNECTION or value in (None, "", False, [], "default"):
            continue
        if parser.defaults.get(dest) == value:
            continue
        used.append(dest)
    if len(positional) == 1:
        used.append("<file>")
    return " ".join(used)


def main():
    parser = reference_parser()
    count = Counter()
    example = {}
    where = defaultdict(str)
    unparsable = 0
    for path in files(TRUNK):
        try:
            lines = path.read_text(errors="replace").splitlines()
        except OSError:
            continue
        for line in lines:
            found = COMMAND.search(line)
            if not found:
                continue
            try:
                args = tokens(found.group(1))
            except ValueError:
                unparsable += 1
                continue
            key = shape(parser, args)
            if key is None:
                unparsable += 1
                continue
            count[key] += 1
            # Prefer an example without unresolved variables.
            literal = not any("$" in a for a in args)
            if key not in example or (literal and "$" in " ".join(example[key])):
                example[key] = args
                where[key] = str(path.relative_to(TRUNK))
    for key, n in count.most_common():
        print("%d\t%s\t%s\t%s" % (n, key, shlex.join(example[key]), where[key]))
    print("# %d shapes, %d invocations, %d not parsable" % (len(count), sum(count.values()), unparsable), file=sys.stderr)


if __name__ == "__main__":
    main()
