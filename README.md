# ncrust

`netconf-console`: a NETCONF command line client, a Rust port of `bin/netconf-console.py` of the
parent `trunk` repository. It takes the same options, sends the same requests and prints the same
output, without Python, `paramiko` or `xmllint`.

Status: the port is feature complete and checked against the Python script (see
[plan/reports](plan/reports)). It is not yet installed in `trunk` in place of the script
(phase P6 of [plan/IMPLEMENTATION_PLAN.md](plan/IMPLEMENTATION_PLAN.md)).

## Build

Rust 1.91.1 is pinned in `rust-toolchain.toml`; `rustup` installs it on first use.

```sh
cargo build --release          # target/release/netconf-console
cargo build                    # debug build, target/debug/netconf-console
```

SSH comes from the `russh` crate and is on by default. `--no-default-features` builds a client
that only speaks `--proto tcp`.

## Use

The command line is the one of the Python script. Connection options come first, then one
command (or a session file):

```sh
# SSH, the default: localhost:2022, user admin, password admin
netconf-console --get-config -x /devices/device

# NETCONF over the NSO/ConfD IPC port
netconf-console --proto tcp --port 2023 --get-config

netconf-console --host nso.example.com --port 830 -u oper -p secret --get -x /interfaces
netconf-console --get-data --db operational --with-origin -x /interfaces
netconf-console --edit-config changes.xml --db candidate
netconf-console --rpc request.xml
netconf-console -s all --iter 3 --get
netconf-console --create-subscription NETCONF -s raw
netconf-console session.xml            # a complete session script; '-' reads standard input
netconf-console -i                     # interactive
netconf-console --help
```

Output styles (`-s`): `pretty` (default, formatted like `xmllint --format`), `plain`, `raw`
(the bytes on the wire without framing), `all`, `last`, `noaaa`.

Options that only ncrust has:

| Option | Meaning |
|---|---|
| `--known-hosts FILE` | the server's host key must be listed in this OpenSSH `known_hosts` file |
| `--host-key-fingerprint SHA256:...` | the server's host key must have this fingerprint |
| `--ssh-kex`, `--ssh-host-key-algs`, `--ssh-ciphers`, `--ssh-macs` | algorithm lists in OpenSSH syntax: `a,b` replaces the default, `+a`, `-a`, `^a` append, remove, put first |

By default any host key is accepted, as in the Python script. With `--debug` the negotiated SSH
algorithms are printed on stderr. Post-quantum key exchange (`mlkem768x25519-sha256`) is available
on request with `--ssh-kex`; see [plan/pqc-extension-points.md](plan/pqc-extension-points.md).

Environment:

| Variable | Effect |
|---|---|
| `TAILF_NETCONF_FORCE_TCP` | when set (to anything), use `--proto tcp --port 2023` |
| `NETCONF_CONSOLE_XMLLINT=1` | format with the installed `xmllint` instead of the built-in formatter |

Errors: problems with the connection, authentication or an option value go to stdout with exit
code 1 (as in Python); protocol errors go to stderr with exit code 1; a bad command line goes to
stderr with exit code 2. Ctrl-C while waiting for the server closes the connection and exits 0.
The table is in [plan/DEVIATIONS.md](plan/DEVIATIONS.md), together with every place where ncrust
behaves differently from the script on purpose.

## Test

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo deny check                 # licences, advisories, bans, sources
```

`cargo test` needs nothing but this repository: the NETCONF and SSH peers are in-process fakes
(`tests/common`). Some tests are compared with files made from the Python script
(`tests/golden`, `tests/fixtures/pretty`); regenerate them with the scripts in `tools/` (they
need `NC_PY_REF=/path/to/netconf-console.py`, by default `../bin/netconf-console.py`).

Against a real server (ConfD or NSO, for example
`examples.confd/intro/1-2-3-start-query-model`):

```sh
tests/system/smoke.sh                                   # NC_PROTO=ssh NC_PORT=2022 for SSH
NC_PY_REF=$PWD/../bin/netconf-console.py tests/differential/run.py     # ncrust against Python
NC_PY_REF=... tests/differential/coverage.py            # options the corpus does not use
NC_PY_REF=... tests/perf/bench.py --release             # time and memory against Python
tools/fuzz_long.sh 3600                                 # long property-test run
```

Comparing over SSH needs a Python with `paramiko`.

## Layout

```
src/cli          command line: optparse rules (legacy.rs), clap struct, validation
src/config.rs    the validated description of a run
src/framing      NETCONF framing 1.0 and 1.1 (no I/O)
src/protocol     hello, request builders, XML writer
src/session      hello exchange, sending, receiving
src/transport    TCP and SSH (ssh/ holds all algorithm and key handling)
src/runner       runs an invocation: replies, subscriptions, interactive mode
src/output       output styles, built-in formatter, schema and AAA filters
tests/           unit-style, end-to-end, golden, differential, robustness tests
plan/            plan, decisions, deviations, reports
```

Which module may use which is checked by `tests/architecture.rs`.
