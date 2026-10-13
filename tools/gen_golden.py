#!/usr/bin/env python3
"""Regenerates tests/golden/{hello,requests}/*.xml from the Python reference implementation.

Usage: NC_PY_REF=/path/to/trunk/bin/netconf-console.py tools/gen_golden.py
(defaults to ../bin/netconf-console.py, i.e. ncrust checked out inside trunk).
"""
import importlib.util
import json
import os
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parent.parent
REF = Path(os.environ.get("NC_PY_REF", ROOT.parent / "bin" / "netconf-console.py"))

spec = importlib.util.spec_from_file_location("netconf_console_ref", REF)
ref = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ref)

cases = json.loads((ROOT / "tests" / "golden" / "cases.json").read_text())
out = ROOT / "tests" / "golden"
(out / "hello").mkdir(exist_ok=True)
(out / "requests").mkdir(exist_ok=True)

for case in cases["hello"]:
    (out / "hello" / (case["name"] + ".xml")).write_text(ref.hello_msg(case["versions"]))

with tempfile.TemporaryDirectory() as tmp:
    for case in cases["requests"]:
        subtree_path = ""
        if "subtree" in case:
            subtree_path = os.path.join(tmp, case["name"] + ".xml")
            Path(subtree_path).write_bytes(case["subtree"].encode("utf-8"))
        opts = SimpleNamespace(
            rpc_attribute=case.get("rpc_attribute", ""),
            xpath=case.get("xpath", ""),
            subtree_filter=subtree_path,
            wdefaults=case.get("with_defaults", ""),
            winactive=case.get("with_inactive", False),
            config_filter=None,
            max_depth="",
            with_origin=None,
            origin_filter=[],
            negated_origin_filter=[],
        )
        db = case.get("db")
        msg = ref.get_msg(case["cmd"], db, opts).replace("<<<MSGID>>>", "1")
        (out / "requests" / (case["name"] + ".xml")).write_text(msg)

print("generated", len(cases["hello"]), "hello and", len(cases["requests"]), "request files", file=sys.stderr)
