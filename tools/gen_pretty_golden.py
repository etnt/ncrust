#!/usr/bin/env python3
"""Regenerates tests/fixtures/pretty/expected from the real `xmllint --format`.

Each case in cases.json gets `expected/<name>.out` with xmllint's stdout, or an empty
`expected/<name>.invalid` if xmllint rejects the input. Also records the xmllint version.
"""
import json
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent.parent / "tests" / "fixtures" / "pretty"
cases = json.loads((HERE / "cases.json").read_text())["cases"]
out_dir = HERE / "expected"
shutil.rmtree(out_dir, ignore_errors=True)
out_dir.mkdir()

version = subprocess.run(["xmllint", "--version"], capture_output=True, text=True)
(HERE / "XMLLINT_VERSION").write_text(version.stderr.splitlines()[0] + "\n")

for case in cases:
    xml = (HERE / case["file"]).read_bytes() if "file" in case else case["xml"].encode()
    proc = subprocess.run(["xmllint", "--format", "-"], input=xml, capture_output=True)
    if proc.returncode == 0:
        (out_dir / (case["name"] + ".out")).write_bytes(proc.stdout)
    else:
        (out_dir / (case["name"] + ".invalid")).write_bytes(b"")

print("generated", len(cases), "expectations with", version.stderr.splitlines()[0], file=sys.stderr)
