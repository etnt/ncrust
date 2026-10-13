# ncrust – Implementation Plan

Implements the design in [PLAN.md](PLAN.md): a Rust replacement for
`bin/netconf-console.py` (in the parent `trunk` repository), shipped as the binary
`netconf-console`. This repository (`ncrust`) is separate from `trunk`; it holds the crate,
its tests and CI. Integration into `trunk` (packaging, lux switch-over) is the last phase.

## Conventions

- **Task IDs** are `P<phase>.<n>`. Each task has a *Verify* line: a command or observable
  result that proves it is done. A task is not done until its Verify passes.
- **Phase exit criteria** must all hold before the next phase starts. Phases build on each other
  but P3 (SSH) can run in parallel with P2 once P1 is complete.
- **Reference implementation**: the Python script. Set `NC_PY_REF=/path/to/trunk/bin/netconf-console.py`
  for differential tests. Python behaviour wins unless listed under "Known defects" in PLAN.md.
- **Test peer**: a running ConfD/NSO (netsim) from `trunk` for system tests; the in-process mock
  server (P1.9) for everything else, so CI in this repo needs no `trunk` build.
- **Quality gates for every task** (enforced from P0 in CI): `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- No task adds features beyond the Python script, except the explicitly listed SSH algorithm /
  host-key options and the escaping/byte-length fixes.

## Phase overview

| Phase | Name | Result |
|---|---|---|
| P0 | Project foundation | Buildable crate, CI, error and config types, decisions recorded |
| P1 | Protocol core (no SSH) | Framing, sessions, TCP transport, `--get*`/`--rpc` over TCP, raw/plain output |
| P2 | Full operations and output | All operations, all styles, subscriptions, interactive, file sessions |
| P3 | SSH transport | russh backend, auth, call-home, algorithm and host-key policy |
| P4 | CLI compatibility | optparse quirks, help text, env vars, exit codes |
| P5 | Differential and system validation | Rust matches Python on a corpus; lux suites pass |
| P6 | Packaging and switch-over in trunk | Built and shipped by trunk, wrapper prefers Rust |
| P7 | PQC enablement | Hybrid KEX available and defaulted, signature path prepared |

---

## P0 – Project foundation

Goal: an empty but fully gated project with all cross-cutting types in place.

| ID | Task | Verify |
|---|---|---|
| P0.1 | Resolve open questions from PLAN.md §10 (crate crypto provider/FIPS, PQ roadmap of server, xmllint exactness, host-key default, platforms). Record answers in `plan/DECISIONS.md` (one entry per question: decision, date, rationale). | File exists; no question left "open". |
| P0.2 | `cargo init`: package `ncrust`, `lib.rs` + `main.rs`, bin name `netconf-console`, `rust-toolchain.toml` pinned, `edition = "2021"` or newer, `Cargo.lock` committed, `.gitignore`. | `cargo build` succeeds; `target/debug/netconf-console` runs and exits 0 on `--help` stub. |
| P0.3 | CI pipeline (fmt, clippy, test, `cargo deny`/license check) for Linux and macOS. | CI green on a no-op commit; license check fails when a disallowed license is added in a trial branch. |
| P0.4 | Create the module tree from PLAN.md §3 as empty modules with doc comments stating each module's single responsibility and allowed dependencies. | `cargo build` passes; a `tests/architecture.rs` test (parsing `use` lines or using `cargo-modules`/`cargo-deny` rules) fails if `framing` or `protocol` import `transport`, or `cli` imports `runner`. |
| P0.5 | `error.rs`: `Error` enum (`thiserror`) with `Stream::{Stdout,Stderr}` and `exit_code()`; `main.rs` is the only place that prints errors and exits. | Unit tests: Stdout-kind error prints on stdout, exit 1; Stderr-kind on stderr, exit 1; `EPIPE` on stdout exits silently. |
| P0.6 | `config.rs`: `Invocation`, `ConnectionSpec`, `Action`, `OutputStyle`, `Db`, `Filter`, `InputSource` as plain data (no I/O). | Compiles; every Python option from `build_options_parser()` maps to a field or is listed in `plan/option-map.md` with its Rust field. |
| P0.7 | `plan/option-map.md`: table of every Python option (name, short form, dest, default, type, which Action uses it). | Reviewed against `grep add_option bin/netconf-console.py`; counts match (script check in P4.1 re-verifies). |
| P0.8 | Test support crate/module: `testutil` with helpers for canonical XML comparison and temp files. | Used by at least one passing test. |

Exit criteria: CI green; architecture test in place; DECISIONS.md complete; option map reviewed.

---

## P1 – Protocol core (no SSH)

Goal: talk NETCONF to a server over the TCP IPC transport, for `--hello`, `--get`, `--get-config`,
`--rpc`, with `raw` and `plain` output. No XML pretty-printing yet.

| ID | Task | Verify |
|---|---|---|
| P1.1 | `framing::eom` (1.0) sans-IO `Decoder`/`Encoder`. Handles marker split across reads, trailing bytes after marker, back-to-back messages. | Unit tests plus `proptest`: for random message sets and every split position, decoded messages equal input. |
| P1.2 | `framing::chunked` (1.1) `Decoder`/`Encoder`, byte-length chunk sizes, limits (size 1..=2^32-1, size field ≤ 10 digits), typed `FramingError::{BadHeader, BadSize, UnexpectedEof}`. | Unit tests mirror each Python return code path (-1 EOF, -2 framing error, EOM); non-ASCII payload test proves byte length (Python defect). Property test with random chunking. |
| P1.3 | `transport::Transport` trait (blocking `read`, `write_all`, `flush`, `set_read_timeout`, `close`) and `Session` byte buffering on top of it. | `Session` tested with an in-memory fake transport that returns data in 1-byte reads and with timeouts. |
| P1.4 | `transport::connect`: resolve host, try all addresses (v4/v6), error text `Failed to connect to {host}: {err}`. | Test: unreachable port yields exactly that message on stdout path (Error kind Stdout). |
| P1.5 | `transport::tcp`: send IPC preamble `[user;ip;tcp;uid;gid;suplgids;cwd;groups;]\n` (cwd from `$PWD`, default `/tmp`); `--groups`, `--sup-groups`. | Mock server test asserts preamble bytes byte-for-byte against Python output captured with a fake server. |
| P1.6 | `protocol::ns`, `protocol::xmlw` (small hand-written writer that escapes text and attribute values; no XML crate needed for writing), `protocol::message_id`. | Unit tests: special characters in attribute/text are escaped; XML declaration and namespace output match Python templates after canonicalisation. |
| P1.7 | `protocol::hello`: build client hello for versions {1.0, 1.1}; parse server hello into `Capabilities` (using a real XML parser, `roxmltree`); version/framing negotiation rules incl. "file session defaults to 1.0 and sends no hello". | Table-driven tests: both, only 1.0, only 1.1 (`-v`), server without 1.1, malformed hello (error text parity with Python). |
| P1.8 | `Session::connect_and_hello`, `send_rpc`, `recv_message`, `recv_chunk(timeout)`, `close` (sends `close-session`, reads reply). | Mock-server test: full hello/rpc/close exchange in both framings. |
| P1.9 | `tests/mock_server.rs`: in-process NETCONF peer (TCP+preamble) scripted per test; supports both framings, split writes, mid-message EOF, framing errors, delays. | Used by P1.8 tests; self-test shows it can replay a recorded transcript. |
| P1.10 | `protocol::ops` for `get`, `get-config`, `Filter` (xpath with quote handling, subtree from `InputSource`), `with-defaults`, `with-inactive`, `rpc-attribute`. | Golden tests: for a set of arg combinations, request XML equals Python output (canonical XML compare). Golden files generated by `tools/gen_golden.py` (runs Python builders). |
| P1.11 | `input::InputSource` (file, `-` via dup of stdin) with chunked reader (16 KiB). | Unit tests with temp file and piped stdin. |
| P1.12 | `runner` minimal: `Hello`, `Get`, `GetConfig`, `Rpc(file)` actions; `raw` and `plain` styles; `--iter`; closing the session. | System test against mock server: output bytes equal expected; `--iter 3` sends 3 requests with ids 1,2,3. |
| P1.13 | Minimal CLI (clap) for the above options only, building `Invocation`. | `netconf-console --proto tcp --port N --get-config --db running -s raw` works against mock server (integration test via `assert_cmd`). |
| P1.14 | Smoke test against a real ConfD/NSO TCP port (documented script `tests/system/smoke.sh`, not part of CI). | Script run output attached in PR: hello capabilities and a `get-config` reply received. |

Exit criteria: P1 tests green in CI; smoke script succeeds against a real server; architecture test passes.

---

## P2 – Full operations and output

Goal: every action and output style of the Python script works over TCP.

| ID | Task | Verify |
|---|---|---|
| P2.1 | `get-data` (NMDA): datastore, subtree/xpath filters, `--config-filter`, `--max-depth`, `--with-origin`, `--origin-filter`, `--negated-origin-filter`, with-defaults placement difference vs `get-config`. | Golden tests vs Python for ≥ 15 argument combinations. |
| P2.2 | `edit-config`, `edit-data`, `copy-config` streaming from file/stdin inside generated envelope, `--trace-id` (with correctly quoted namespace), `--with-inactive`. | Mock server records bytes; reassembled XML equals golden; large (>1 MiB) file passes with 16 KiB reads; test asserts the trace-id fix. |
| P2.3 | Simple ops: `kill-session`, `discard-changes`, `commit`, `validate`, `copy-running-to-startup`, `get-schema`. | Golden tests vs Python. |
| P2.4 | Subscriptions: `create-subscription`, `establish-subscription`, `kill-subscription`, `--start-time/--replay-start-time`, `--stop-time`. Stream-filter builder (xpath/subtree) with escaping. | Golden tests; mock server test: notifications stream until EOF; stop-time ends the loop after `replayComplete`; timestamps parsed as UTC. |
| P2.5 | `runner::plan`: translate `Action` to `Plan` (request source, reply count, default style, hello printing, close behaviour) as a pure function. | Table test covering every Action incl. the "n" and style defaults from Python (`all` for subscriptions/interactive, `pretty` otherwise, `hello`+`raw` → n=0). |
| P2.6 | `runner::replies`: reply loop with 1 s poll timeout, EOF/framing errors with exact messages, "No data received in the reply.", Ctrl-C exits 0 after closing. | Mock server tests per branch. |
| P2.7 | `output::pretty`: libxml2-compatible `--format` formatter. | Differential test: ≥ 200 reply fixtures (captured from ConfD/NSO, including mixed content, CDATA, namespaces, comments, empty elements, entities, non-ASCII) produce byte-identical output to `xmllint --format`. Fixtures live in `tests/fixtures/replies/`. |
| P2.8 | `output::noaaa` awk-compatible filter. | Differential test vs the Python `awk` pipeline on fixtures with `aaa`/`nacm` blocks. |
| P2.9 | Styles `pretty`, `all`, `last`, `noaaa` wired into `runner` (print policy per Python comment table). | Table test of (style × n × replies) → expected printed replies. |
| P2.10 | Optional `NETCONF_CONSOLE_XMLLINT=1` escape hatch that pipes through external `xmllint`. | Test (skipped if `xmllint` missing) shows identical output to built-in path on fixtures. |
| P2.11 | `get-schema` output path: extract `<data>` text from `rpc-reply`, error messages for non-`data` replies. | Mock server tests, three failure messages match Python text. |
| P2.12 | File session mode (`<filename>`): raw send without framing wrapper, read until EOF, 1.0 default. | Mock server test with a hand-written session file. |
| P2.13 | `interactive` mode: prompt text, empty-line handling, ids starting at 2. | `assert_cmd` test feeding stdin; output equals Python for a scripted session. |

Exit criteria: for every Python Action, a golden/mock test exists and passes; pretty-printer corpus is 100 % identical.

---

## P3 – SSH transport

Goal: SSH client with password/public-key auth, call-home, configurable algorithms.
Can start once P1 is complete; P3 tasks do not touch `protocol/` or `runner/` except for P3.9.

| ID | Task | Verify |
|---|---|---|
| P3.1 | `transport::ssh::backend`: `SshBackend`, `Channel`, `SshOptions`, `AuthMethod`, `AlgorithmPolicy`, `HostKeyPolicy`, `BackendCaps` (algorithm names are strings). | Compiles; a fake backend implements the trait and drives `SshTransport` in unit tests. |
| P3.2 | `ssh::russh_backend`: connect over an existing `TcpStream`, open `netconf` subsystem, blocking adapter over a private current-thread runtime, read timeout support. | Integration test against a local SSH server fixture (see P3.3) performing hello/get. |
| P3.3 | SSH test server fixture (e.g. in-process russh server or containerised OpenSSH with a netconf subsystem stub). | Test starts/stops it deterministically; usable in CI without root. |
| P3.4 | Auth: password; private key from file with type auto-detection (`keys.rs`); `--privKeyType/--privKeyFile` accepted (mismatch → warning). Error text `Authentication failed.` on Stdout stream. | Tests: password ok/bad, RSA/Ed25519/ECDSA key ok, passphrase-protected key error, wrong-type hint warning. |
| P3.5 | `HostKeyPolicy`: `AcceptAny` (default per P0.1), `--known-hosts FILE`, `--host-key-fingerprint SHA256:…`. | Tests: matching/mismatching keys accepted/rejected; default behaviour matches Python (accept). |
| P3.6 | Algorithm policy: `--ssh-kex`, `--ssh-host-key-algs`, `--ssh-ciphers`, `--ssh-macs` with OpenSSH `+ - ^` list semantics; `BackendCaps` validation with clear errors; negotiated algorithms printed under `--debug`. | Unit tests for list semantics; integration test negotiates a forced algorithm; unsupported name yields a clear error before connecting. |
| P3.7 | Call-home: `--listen`, `--listen-address`, `--listen-port` (accept one connection, print `** listening on`/`** call home from` lines exactly as Python), then run SSH client over the accepted socket. | Test connects a fixture server to the listener and completes hello; stdout lines match Python. |
| P3.8 | Write-batching behaviour: avoid tiny SSH packets for streamed file payloads (16 KiB buffering as in `NetconfSSH._send`). | Test counts channel writes for a 1 MiB payload: ≤ ⌈size/16 KiB⌉ + small constant. |
| P3.9 | Wire SSH into CLI/`runner` (`--proto ssh` default, `--user/--password/--host/--port`). | `assert_cmd` end-to-end test over SSH fixture matches the TCP result for the same requests. |
| P3.10 | Document PQC extension points in `plan/` (where a new KEX/host-key/signature is added: `algorithms.rs`, `keys.rs`, backend). Verified by P7. | Reviewed doc; P7 tasks refer to it. |
| P3.11 | (Conditional on P0.1) `subprocess_backend` using system `ssh -s netconf`, password via `SSH_ASKPASS`. | If enabled: same E2E test passes with the backend selected via feature flag; call-home reports "unsupported". |

Exit criteria: SSH E2E tests green on Linux and macOS CI; no `match` on key/algorithm types outside `transport/ssh/`
(architecture test extended).

---

## P4 – CLI compatibility

Goal: command lines that work with the Python script work unchanged.

| ID | Task | Verify |
|---|---|---|
| P4.1 | Complete clap definition for all options with identical names, shorts, defaults, and option groups; help text ported. | Script `tests/cli_options.rs` extracts option names from `$NC_PY_REF --help` and asserts every one is accepted by the Rust binary. |
| P4.2 | Legacy layer: optional value for `--get-config`/`--get-data`; greedy `-x/--xpath`; abbreviated long options; `--opt=value`; `-` as filename; `-v/--version` meaning NETCONF version. | Table test (≥ 40 argument vectors) comparing the parsed `Invocation` against expectations derived from Python's parsed `o`. |
| P4.3 | `validate.rs`: all Python validation (`--with-defaults` values, origin values, listen only with ssh, "a filename or a command option is required", conflicts). Error messages/streams identical. | Table test of invalid vectors: stdout/stderr text and exit code equal Python. |
| P4.4 | `TAILF_NETCONF_FORCE_TCP` handling (forces tcp + port 2023, as in Python). | Test with env var set. |
| P4.5 | `--debug` (prints pid line as Python), `-s raw` trace printing of received bytes. | Test compares output prefix to Python. |
| P4.6 | Exit-code and stream contract documented and tested: Stdout errors → stdout/1, Stderr errors → stderr/1, Ctrl-C → 0, broken pipe silent. | Tests incl. `SIGINT` via `assert_cmd` + child signalling and `| head -1` pipe. |
| P4.7 | `--help` output reviewed (not required byte-identical; must list all options and groups). | Review sign-off recorded in the PR. |

Exit criteria: P4.1–P4.3 tables pass; no unexplained deviation in `plan/DEVIATIONS.md` (new file listing intentional differences, e.g. leading space in xpath, unquoted trace-id).

---

## P5 – Differential and system validation

Goal: show equivalence to the Python implementation on realistic workloads.

| ID | Task | Verify |
|---|---|---|
| P5.1 | Differential harness `tests/differential/`: runs an argument-vector list against Python and Rust against a live server, diffs stdout, stderr, exit code. Server address via env. | Harness self-test with identical commands on Python vs Python reports no diff. |
| P5.2 | Command corpus: derive from lux files in `trunk` (`grep netconf-console` over `*.lux`, `*.luxinc`, `*.sh`, Makefiles) – unique option combinations collected in `tests/differential/commands.txt`. | Corpus covers every option at least once; coverage script lists unused options. |
| P5.3 | Run the corpus over TCP and SSH against a netsim/ConfD from `trunk`. | Report in `plan/reports/diff-<date>.md`: 0 unexplained diffs; each explained diff has a DEVIATIONS.md entry. |
| P5.4 | Run `trunk` lux suites that use netconf-console with the Rust binary (via the wrapper switch in P6.3, usable locally before P6 completes). | Lux result summary: pass/fail identical to Python baseline run; failing cases triaged. |
| P5.5 | Performance and robustness: 100 MiB `get-config` reply, 1 M-element edit file, 10 000 `--iter`; memory stays bounded for streamed input; no slower than Python. | Benchmark script output saved; thresholds recorded. |
| P5.6 | Fuzz framing decoders and hello parser (`cargo fuzz` or proptest at high case count). | 1 hour fuzz run with no panics; corpus checked in. |
| P5.7 | Security review: XML escaping coverage, no password in logs, safe temp handling, dependency audit (`cargo audit`). | Checklist in `plan/reports/security.md` with all items checked; `cargo audit` clean or waivers listed. |

Exit criteria: differential report clean, lux parity achieved, fuzz and audit clean.

---

## P6 – Packaging and switch-over in trunk

Goal: `trunk` builds and ships the Rust binary; Python script becomes the fallback.

| ID | Task | Verify |
|---|---|---|
| P6.1 | Decide integration form (git submodule, subtree, or fetched via `deps/ncrust_version`), consistent with other `deps/*_version` entries. | Decision recorded in DECISIONS.md; `trunk` builds fetch it. |
| P6.2 | Toolchain in `trunk` dev env (`support/nix/devshell.nix`, `shell.nix`, `flake.nix`) and offline vendored crates. | Fresh `nix` shell builds the binary without network after vendoring. |
| P6.3 | Makefile target (per `makefile.instructions.md`) that builds `--release` and installs `bin/netconf-console`; wrapper chooses `NETCONF_CONSOLE_IMPL=rust|python` (default python until P6.6). `netconf-console-tcp` unchanged. | `make` in the relevant dir builds; wrapper runs either implementation. |
| P6.4 | Install into `confd_dir/bin`, `ncs_dir/bin`, `ncs_dir/netsim/confd/bin` by following the existing `netconf-console.py` copy rules. | Release build contains the binary in all three; `file` shows correct architecture. |
| P6.5 | SBOM (`SBOM.spdx.in`) and `licenses/` updated with all Rust crates. | SBOM generation passes; licence check script clean. |
| P6.6 | Flip default to Rust after a full lux run on all supported platforms is green. | CI report for each platform; wrapper default changed in a single commit. |
| P6.7 | Release notes / CHANGES entry (use the CHANGES Note Writer agent); document `NETCONF_CONSOLE_IMPL` and new SSH options. | Entry present; reviewed. |
| P6.8 | One-release deprecation of the Python script, then removal plan with ticket. | Jira ticket created for removal; date set. |

Exit criteria: trunk release build ships the Rust binary as default; fallback documented.

---

## P7 – PQC enablement

Goal: use post-quantum SSH algorithms without structural changes.

| ID | Task | Verify |
|---|---|---|
| P7.1 | Inventory: PQ algorithms supported by the NSO/ConfD SSH server (see `lib/confd_ssh`) and by the chosen russh/crypto-provider version. Write `plan/pqc-matrix.md`. | Matrix lists KEX, host-key and user-auth algorithms with server/client support status. |
| P7.2 | Enable hybrid KEX (`mlkem768x25519-sha256`, and `sntrup761x25519-sha512` if the server supports it) in `AlgorithmPolicy` defaults, ahead of classical KEX. | Test against a PQ-capable server: negotiated KEX name asserted in `--debug` output; fallback to classical when the server lacks PQ (test with classical-only server). |
| P7.3 | Policy option to require PQ (`--ssh-kex mlkem768x25519-sha256` only) fails clearly when the server lacks it. | Test: connection fails before auth with an explicit error message. |
| P7.4 | Add PQ host-key and user-key algorithm names via `algorithms.rs`/`keys.rs` when the SSH standards and server support land. | New names added with no change outside `transport/ssh/`; architecture test confirms. |
| P7.5 | FIPS/CiscoSSL build variant if required by P0.1 (crypto provider feature). | Build with the feature; `ldd`/symbol check shows the intended provider; PQ matrix updated. |
| P7.6 | Interop CI job against the latest OpenSSH (PQ default) and NSO. | Job green; results stored in `plan/reports/`. |

Exit criteria: PQ KEX negotiated end-to-end with fallback verified; no structural change needed to add an algorithm (demonstrated by P7.4).

---

## Cross-cutting definition of done (per PR)

1. New/changed behaviour has a unit or component test; bug-fix-like deviations have a test and a
   `DEVIATIONS.md` entry.
2. Quality gates pass (fmt, clippy, tests, architecture test).
3. No new dependency without licence and maintenance review (recorded in PR description).
4. Public items in `lib.rs` modules have one-line doc comments describing responsibility.

## Risk register

| Risk | Phase | Mitigation |
|---|---|---|
| `xmllint --format` byte-exactness | P2 | Large differential corpus (P2.7), external-xmllint escape hatch (P2.10) |
| russh lacks required PQ KEX / FIPS crypto | P3, P7 | Backend trait; subprocess backend (P3.11); crypto-provider feature (P7.5) |
| optparse behaviours used by scripts but unknown | P4, P5 | Corpus extracted from real usage in `trunk` (P5.2) |
| Rust toolchain and vendoring in trunk build | P6 | Early nix/offline-build task (P6.2); can start earlier in parallel |
| Blocking-over-async adapter bugs (timeouts, backpressure) | P3 | Dedicated tests P3.2, P3.8; confined to `ssh/` |
