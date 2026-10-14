# ncrust

`ncrust` is a NETCONF command line client. It is a Rust port of `netconf-console.py` and takes
the same options, sends the same requests and prints the same output, without Python, `paramiko`
or `xmllint`. It is a general purpose NETCONF tool; if it is started under another name (for
example through a link called `netconf-console`) the usage messages show that name.

Status: the port is feature complete and checked against the Python script (see
[plan/reports](plan/reports)). The work plan is in
[plan/IMPLEMENTATION_PLAN.md](plan/IMPLEMENTATION_PLAN.md).

## Build

Rust 1.91.1 is pinned in `rust-toolchain.toml`; `rustup` installs it on first use.

```sh
cargo build --release          # target/release/ncrust
cargo build                    # debug build, target/debug/ncrust
```

SSH comes from the `russh` crate and is on by default.

### Cargo features

| Feature | Default | Meaning |
|---|---|---|
| `russh` | on | the SSH transport |
| `tailf-compat` | off | Tail-f NSO and ConfD support: `--proto tcp` (their IPC port) with `-g`/`-G`, `-s noaaa`, `--with-inactive`, `--trace-id`, `--with-defaults true\|false`, `TAILF_NETCONF_FORCE_TCP`, and the defaults of `netconf-console.py` (port 2022, user and password `admin`) |

```sh
cargo build --release --features tailf-compat   # what NSO and ConfD build
```

Without `tailf-compat` the defaults are those of a general NETCONF client: port 830, the user
running the program, an empty password.

## Use

The command line is the one of the Python script. Connection options come first, then one
command (or a session file):

```sh
# SSH: localhost:830 (with --features tailf-compat: localhost:2022, user admin, password admin)
ncrust --get-config -x /devices/device

# With --features tailf-compat: NETCONF over the NSO/ConfD IPC port
ncrust --proto tcp --port 2023 --get-config

ncrust --host nso.example.com --port 830 -u oper -p secret --get -x /interfaces
ncrust --get-data --db operational --with-origin -x /interfaces
ncrust --edit-config changes.xml --db candidate
ncrust --rpc request.xml
ncrust -s all --iter 3 --get
ncrust --create-subscription NETCONF -s raw
ncrust session.xml            # a complete session script; '-' reads standard input
ncrust -i                     # interactive
ncrust --help
```

Output styles (`-s`): `pretty` (default, formatted like `xmllint --format`), `plain`, `raw`
(the bytes on the wire without framing), `all`, `last`, and with `tailf-compat` `noaaa`.

Options that only ncrust has:

| Option | Meaning |
|---|---|
| `--known-hosts FILE` | the server's host key must be listed in this OpenSSH `known_hosts` file |
| `--host-key-fingerprint SHA256:...` | the server's host key must have this fingerprint |
| `--ssh-kex`, `--ssh-host-key-algs`, `--ssh-ciphers`, `--ssh-macs` | algorithm lists in OpenSSH syntax: `a,b` replaces the default, `+a`, `-a`, `^a` append, remove, put first |

By default any host key is accepted, as in the Python script. With `--debug` the negotiated SSH
algorithms are printed on stderr. The post-quantum hybrid key exchange `mlkem768x25519-sha256`
is tried first, so a server that has it (ConfD and NSO do, as does OpenSSH 10) is reached with it
and any other server with a classical exchange; `--ssh-kex` selects the algorithms explicitly.
See [plan/pqc-matrix.md](plan/pqc-matrix.md) and
[plan/pqc-extension-points.md](plan/pqc-extension-points.md).

Environment (the first only with `tailf-compat`):

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
cargo clippy --all-targets --features tailf-compat -- -D warnings
cargo test --features tailf-compat
cargo deny check                 # licences, advisories, bans, sources
```

The tests that need the TCP transport, `noaaa` or the Python defaults (the mock-server based
runner and CLI tests, the golden files made from the Python script) only run with
`--features tailf-compat`. The scripts under `tests/system`, `tests/differential` and `tests/perf`
need a binary built with it.

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

## License

ncrust is licensed under the [Mozilla Public License 2.0](LICENSE) (SPDX: `MPL-2.0`). Its
dependencies keep their own licenses; `cargo deny check licenses` verifies that they are
compatible.
