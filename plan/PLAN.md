# Plan: netconf-console in Rust

Port of `bin/netconf-console.py` (1431 lines, Python + paramiko + `xmllint`) to a
single self-contained Rust binary named `netconf-console`.

## 1. Goals / non-goals

Goals
- Drop-in CLI compatibility with the Python script (≈570 `.lux` files and many demo
  scripts call it; see section 9).
- No runtime dependency on Python, paramiko or `xmllint`/`awk`.
- Clear module boundaries: CLI parsing, framing, transport, session, request
  building, output are separate and individually testable.
- SSH crypto is isolated behind one interface so PQC algorithms (hybrid KEX now,
  PQ signatures later) are a configuration/back-end change, not a redesign.

Non-goals (initially)
- New features (NETCONF over TLS, new operations). The structure should allow them.
- Python 2 compatibility quirks.

## 2. What the Python script does (behaviour inventory)

| Area | Python | Notes for the port |
|---|---|---|
| Transport SSH | `NetconfSSH` (paramiko), subsystem `netconf`, password or private key auth, call-home `--listen` | Host key is **never verified** (`publicKey` is always `""`). Key type chosen by `--privKeyType` (default falls to DSS). `base64.decodestring` is dead code. |
| Transport TCP | `NetconfTCP`, sends NSO IPC preamble `[user;ip;tcp;uid;gid;suplgids;cwd;groups;]\n` | Used by `netconf-console-tcp`, `TAILF_NETCONF_FORCE_TCP` forces tcp + port 2023 |
| Framing | `NetconfSSHLikeTransport`: `]]>]]>` (1.0) and chunked (1.1) | Chunk length uses `len(str)` not bytes (bug for non-ASCII). Recv returns `(code, data)` tuples with magic ints. |
| Hello | sends 1.0/1.1 caps, picks 1.1 if both sides advertise it; file input with no `-v` => 1.0 and no client hello sent | Hello parsing walks minidom by hand. |
| Request building | string `format()` of XML, no escaping | `trace_id` emits `xmlns=` without quotes (invalid XML). `mk_stream_filter_str` has two identical branches. Message ids are a mix of `<<<MSGID>>>`, constant `1`, and interactive counter starting at 2. |
| Operations | get, get-config, get-data (NMDA), edit-config, edit-data, copy-config, copy-running-to-startup, kill-session, discard-changes, commit, validate, get-schema, create/establish/kill-subscription, raw `--rpc`, hello-only, interactive, whole-session file | |
| Output styles | raw, plain, pretty, all, last, noaaa; pretty = pipe through `xmllint --format`; noaaa = `awk` filter on top | See section 7. |
| Main loop | `main()` is ~330 lines mixing option dispatch, transport setup, version negotiation, file streaming, reply loop, subscription stop-time/replayComplete, get-schema special path, close | Split by responsibility. |
| CLI | optparse: abbreviated long options, `--get-config [db]` / `--get-data [db]` optional values, `-x` swallowing all following non-option args, option groups | Needs compat layer (section 5). |
| Errors | `StdoutError` -> stdout + exit 1, `StderrError` -> stderr + exit 1, others traceback | Keep stream+exit code behaviour. Ctrl-C during recv exits 0. |

## 3. Crate layout

One package, library + thin binary (a workspace is overkill; a library target still
gives us integration-testable code). Location is an open question (section 10);
suggested `lib/netconf_console/` (or `tools/netconf-console/`).

```
netconf-console/
  Cargo.toml
  Cargo.lock                 # committed; deps vendored (section 8)
  src/
    main.rs                  # parse -> run -> map Error to stream/exit code
    lib.rs
    error.rs                 # Error { kind, stream: Stdout|Stderr }, exit code mapping
    cli/
      mod.rs                 # clap definition, help text/groups
      legacy.rs              # optparse quirks (greedy -x, optional db value, prefixes)
      validate.rs            # RawArgs -> Invocation (all validate_options logic)
    config.rs                # Invocation, ConnectionSpec, Action, OutputStyle (pure data)
    input.rs                 # InputSource: file | '-' (dup stdin) ; chunked reader
    protocol/
      ns.rs                  # namespace/capability constants
      hello.rs               # build client hello, parse server hello -> Capabilities
      message_id.rs          # MessageIdAllocator
      xmlw.rs                # small escaping-safe XML writer (quick-xml Writer)
      ops/                   # one file per operation -> impl Operation
        mod.rs               # trait Operation { fn write_body(&self, w) ; fn expects() }
        get.rs  get_config.rs  get_data.rs  edit_config.rs  edit_data.rs
        copy_config.rs  simple.rs (commit, discard, kill-session, validate, copy-running-to-startup)
        get_schema.rs  subscription.rs (create/establish/kill)  raw_rpc.rs
      filter.rs              # Xpath | Subtree(InputSource) -> filter element
    framing/
      mod.rs                 # trait-free sans-IO types
      eom.rs                 # 1.0 codec
      chunked.rs             # 1.1 codec (byte-length, RFC 6242 limits)
    transport/
      mod.rs                 # trait Transport { read/write/set_read_timeout/close }
      connect.rs             # resolve + connect (all addrs), call-home listen/accept
      tcp.rs                 # NSO IPC preamble transport
      ssh/
        mod.rs               # SshTransport: implements Transport via SshBackend
        backend.rs           # trait SshBackend (section 6)
        russh_backend.rs     # default backend            [cargo feature `russh`]
        subprocess_backend.rs# optional: spawn system `ssh -s netconf` [feature `ssh-subprocess`]
        algorithms.rs        # AlgorithmPolicy: kex / host key / cipher / mac preference lists
        keys.rs              # private key loading, key type detection
        hostkey.rs           # HostKeyPolicy: AcceptAny | KnownHosts | Fingerprint
        auth.rs              # AuthMethod: Password | PublicKey | (future) others
    session.rs               # Session<T: Transport>: hello exchange, framing switch, send/recv message, close
    runner/
      mod.rs                 # run(Invocation) -> Result<()>; builds Plan, drives loop
      plan.rs                # Action -> Plan { request source, reply count, style defaults }
      replies.rs             # reply loop: counts, hello-only, EOF handling, get-schema path
      subscription.rs        # stop-time, replayComplete detection
      interactive.rs         # read_msg loop
    output/
      mod.rs                 # trait ReplySink; Printer by style
      pretty.rs              # xmllint-compatible formatter
      noaaa.rs               # AAA/NACM filter
      schema.rs              # get-schema <data> extraction
  tests/
    golden/                  # (args, expected request XML, expected output) from Python
    mock_server.rs           # in-process NETCONF peer over TCP (both framings)
    cli_compat.rs
```

Dependency direction (no cycles): `cli -> config <- runner -> session -> {framing, transport}`;
`runner -> protocol, output`; `protocol` and `framing` have no I/O.

## 4. Key design decisions

1. **Sans-IO framing.** `framing::Decoder::feed(&[u8]) -> impl Iterator<Item=Frame>` with
   `Frame::{Data(Vec<u8>), EndOfMessage}` and typed `FramingError` instead of
   `(-1, bytes)` / `(-2, bytes)`. Property tests split input at every byte boundary.
   Chunk sizes counted in bytes.
2. **Typed configuration.** `cli::validate` produces an `Invocation` with an
   `Action` enum (`Hello | Get{..} | GetConfig{..} | GetData{..} | EditConfig(Input)
   | EditData(Input) | CopyConfig(Input) | Rpc(Input) | SessionFile(Input) | KillSession(id)
   | Commit | DiscardChanges | Validate(db) | CopyRunningToStartup | GetSchema(id)
   | CreateSubscription{..} | EstablishSubscription{..} | KillSubscription(id) | Interactive`).
   Illegal combinations (e.g. `--listen` with tcp, `--only-config-*` conflicts) are rejected
   once, up front. Nothing downstream looks at raw option values.
3. **Operations as data.** Each op implements `Operation` and writes through an XML
   writer that escapes text/attributes. Fixes the injection/quoting issues; file-based
   payloads (edit/copy) are streamed as raw bytes inside the generated envelope, as today.
4. **Message ids owned by `Session`.** Preserve observable ids: requests built from
   options use 1,2,3… per iteration, `--rpc`/file payloads use 1, interactive starts at 2.
   Verified by golden tests.
5. **Plan/execute split.** `runner::plan` turns an `Action` into: request source, number
   of replies (`n`: 1, `--iter`, until EOF), default style (subscriptions and interactive
   => `all`), whether to print hello reply, whether to close the session. This replaces the
   interleaved `n`/`do_print`/`is_closed` flag logic in Python.
6. **Sync API, async internals only where required.** The CLI is inherently
   sequential. `Transport` is a blocking trait with read timeouts (needed for the 1 s poll
   used by subscription stop-time). The russh backend owns a private current-thread tokio
   runtime and adapts to that blocking trait; tokio does not leak outside `ssh/`.
7. **Errors.** One `Error` enum (`thiserror`) carrying `Stream::{Stdout, Stderr}`; `main`
   is the only place that prints and exits. Handle `EPIPE` quietly (Python-like behaviour
   when piped into `head`).
8. **Security improvements that stay compatible.** Escaping in XML construction; host key
   policy is explicit (`AcceptAny` stays the default to match today's behaviour, with an
   opt-in `--known-hosts`/`--host-key-fingerprint`). Passwords are never logged in `--debug`.

## 5. CLI compatibility (cli/)

Use `clap` (derive) plus a small `legacy.rs` pre/post-processor. Items to cover with
explicit tests, since optparse differs from clap:

- Option groups and help text: Style, SSH, TCP, NETCONF Command options.
- `-v` is `--version` (NETCONF version), not program version; `-h/--help` only.
- Abbreviated long options (`infer_long_args`).
- `--get-config [DB]`, `--get-data [DB]`: optional value taken only if next arg does not
  start with `-`.
- `-x/--xpath`: consumes all following args until the next option, joined with spaces.
  Accept this via `num_args(1..)` + join, but normalise the leading space Python adds.
- Positional `<filename>` or `-`; exactly zero or one.
- `--start-time` and `--replay-start-time` write the same variable in Python (last wins).
  Make them aliases of one argument.
- `--origin-filter` / `--negated-origin-filter` repeatable and validated against
  `default|dynamic|intended|learned|system|unknown`.
- `--with-defaults` accepted values incl. deprecated `true|false` (emitted as `rpc`
  attribute); `TAILF_NETCONF_FORCE_TCP` forces `--proto tcp --port 2023`.
- `--privKeyType` accepted for compatibility; the key type is auto-detected, mismatch is
  a warning, not an error.
- Error text for validation failures kept byte-identical where tests grep for it.

## 6. SSH and PQC-readiness

Everything cryptographic lives under `transport/ssh/` and is reached through:

```rust
trait SshBackend {
    fn connect(&mut self, stream: TcpStream, opts: &SshOptions) -> Result<Box<dyn Channel>>;
    fn capabilities(&self) -> BackendCaps;   // which kex/hostkey/sig algorithms, call-home support
}
struct SshOptions {
    user: String,
    auth: AuthMethod,                // Password | PublicKey(PrivateKey) | extensible
    algorithms: AlgorithmPolicy,     // ordered, name-based (strings), not enums
    host_key: HostKeyPolicy,
}
```

Rules that keep us PQC-ready:
- **Algorithms are names, not enums.** `AlgorithmPolicy { kex, host_key, ciphers, macs }` are
  ordered `Vec<AlgName>` with defaults per backend. New names (`mlkem768x25519-sha256`,
  `sntrup761x25519-sha512@openssh.com`, later ML-DSA based host key/pubkey algorithms)
  need no code change in `cli/`, `session/` or `runner/`.
- **Key handling is a trait boundary** (`keys.rs`): `PrivateKey` is opaque and loaded via
  the `ssh-key` crate (or the backend); no `match` on RSA/DSS/Ed25519 outside `keys.rs`.
  `KeyKind` is `#[non_exhaustive]`.
- **CLI exposes policy, not algorithms.** Add `--ssh-kex`, `--ssh-host-key-algs`,
  `--ssh-ciphers`, `--ssh-macs` (comma lists, `+`/`-`/`^` prefix semantics as in OpenSSH).
  Defaults: hybrid PQ KEX preferred when the backend supports it, classical fallback.
  Negotiated algorithms are printed with `--debug`.
- **Capability probing.** `BackendCaps` lets the CLI fail with a clear message if a
  requested algorithm is unsupported by the chosen backend, instead of an opaque handshake error.
- **Crypto provider is a cargo feature, not hard-wired.** With russh the provider
  (aws-lc-rs vs ring) is selectable at build time. Whether the shipped tool must use the
  FIPS-validated CiscoSSL (`ciscossl/`, includes `ciscoqs_provider`/`ciscossl_oqs`) is a
  product decision; see open questions. The abstraction must allow swapping without touching
  upper layers.
- **Two backends, one trait**:
  - `russh_backend` (default): pure Rust, supports password/pubkey and call-home (we own
    the already-accepted socket). PQ KEX availability must be verified for the chosen
    russh version before committing.
  - `subprocess_backend` (optional fallback): runs `ssh -s <host> netconf` with the system
    OpenSSH, so PQC support follows the host OpenSSH (sntrup761x25519 since 9.0,
    mlkem768x25519 since 9.9). Password via `SSH_ASKPASS` helper (the binary itself).
    No call-home. Only built if russh PQC support lags what NSO needs.
- **Other future protocols** (NETCONF over TLS with PQ groups through CiscoSSL/oqs
  provider) plug in as another `Transport` implementation; `Session` does not change.

PQC milestones: (a) algorithm-name plumbing + `--ssh-kex` in MVP, (b) hybrid KEX on by
default once the NSO/ConfD SSH server side supports it, (c) PQ host-key/user-auth
signatures once standardised in SSH; only `keys.rs`, `algorithms.rs` and backend
change.

## 7. Output compatibility (highest regression risk)

Most lux tests compare output produced by `xmllint --format`. Plan:

1. Implement `output::pretty` as a streaming formatter that reproduces libxml2 `--format`
   (XML declaration kept, 2-space indent, element-only content indented, mixed content left
   untouched, empty elements `<a/>`, entity/CDATA handling, namespace prefixes preserved).
2. Build a differential corpus: capture real replies from ConfD/NSO (all lux-used ops) and
   assert `pretty(reply) == xmllint --format reply` byte for byte in CI (xmllint exists in the
   dev environment; only tests need it).
3. `noaaa`: reimplement the awk logic (line-based: drop from a line matching `<(aaa|nacm)`
   through `</(aaa|nacm)>`), applied to the pretty output, to stay bug-compatible.
4. Keep an escape hatch for the first releases: `NETCONF_CONSOLE_XMLLINT=1` pipes through
   external `xmllint` like Python does. Remove once the corpus is green.
5. `raw` style: bytes exactly as received with framing removed; `plain`: one reply
   unformatted; `last`/`all` semantics from the table in the Python source comment
   preserved and covered by tests.
6. Error messages: `"No data received in the reply."`, `"unexpected EOF in NETCONF
   transport"`, `"NETCONF transport framing error"`, auth failure text, connect failure text.

## 8. Build and packaging

No Rust currently exists in the tree (only Python tooling), so this is new infrastructure:

- Toolchain pinned via `rust-toolchain.toml`; add `cargo`/`rustc` to `support/nix/devshell.nix`
  (it already lists paramiko for the Python version) and `shell.nix`/`flake.nix`.
- Offline builds: commit `Cargo.lock`, vendor crates (`cargo vendor`) or add a
  `deps/<crate-set>_version` entry following the existing `deps/*_version` convention.
- Makefile integration (follow `makefile.instructions.md`): a target that builds
  `--release` and installs to `bin/netconf-console`, wired into the same lists that today
  copy `netconf-console.py` into `confd_dir/bin`, `ncs_dir/bin`,
  `ncs_dir/netsim/confd/bin` (check `config/output_bin.mk`).
- Platforms: whichever the product ships today (Linux x86_64/aarch64, macOS arm64/x86_64);
  static or musl linking only if release policy requires it.
- SBOM (`SBOM.spdx.in`) and `licenses/` must list the Rust crates.
- `bin/netconf-console` shell wrapper stays during transition (prefers the Rust binary,
  else Python); `bin/netconf-console-tcp` unchanged.

## 9. Migration and testing strategy

Test layers
1. Unit: framing (property-based), hello parsing, each `Operation` XML (compared as
   canonicalised XML to Python output), CLI parsing table, pretty/noaaa.
2. Component: `tests/mock_server.rs` — in-process NETCONF peer over TCP+IPC preamble for
   both framings, split reads, 1.1 framing errors, EOF mid-message, subscription with
   `replayComplete` and stop-time, call-home accept.
3. Differential: script that runs a list of arg vectors against the Python script and the
   Rust binary against a running netsim/ConfD and diffs stdout/stderr/exit code.
4. System: run the existing lux suites that use netconf-console with the Rust binary
   (switch via wrapper env var `NETCONF_CONSOLE_IMPL=rust|python`); then flip the default.

Phases
- **P0 – Skeleton and decisions**: crate, CI build, error type, config types; resolve open
  questions below.
- **P1 – Core without SSH**: framing, TCP transport (IPC preamble), session/hello, `Get*`,
  `--rpc`, file input, `raw`/`plain` output. Test against mock server and ConfD TCP port 2023.
- **P2 – Full operation set and output**: all ops, pretty/noaaa/last/all, iterations,
  interactive, subscriptions, get-schema.
- **P3 – SSH**: `SshBackend` + russh backend, password and key auth, call-home,
  algorithm policy options, host key policy.
- **P4 – Compat hardening**: CLI quirk tests, differential corpus, run lux suites.
- **P5 – Packaging and switch**: Makefile/nix/SBOM, wrapper prefers Rust, deprecate Python script.
- **P6 – PQC**: enable hybrid KEX by default per milestones in section 6; optional
  subprocess backend if needed.

## 10. Open questions

1. Where should the crate live (`lib/netconf_console/` vs `tools/`), and is the Rust
   toolchain acceptable in the release build environment (offline/vendored crates)?
2. Must the tool use CiscoSSL/FIPS crypto in FIPS builds? That decides russh crypto
   provider (aws-lc-rs vs ring) versus linking CiscoSSL, and whether the subprocess
   backend is viable.
3. Which PQ algorithms does the NSO/ConfD SSH server (Erlang `confd_ssh`) plan to
   support and when (hybrid KEX only, or also PQ host keys / user auth)? Drives milestone (b)/(c).
4. Is byte-exact `xmllint --format` output required for all styles, or only for what lux
   tests check? (Affects how much of libxml2 formatting we emulate.)
5. Keep accepting any host key by default, or switch to a verifying default with a
   compat flag (behaviour change for demos/tests)?
6. Which platforms/targets must the binary support?

## 11. Known Python defects to fix rather than copy

- Unescaped values in generated XML (xpath, stream, identifiers, db, ids).
- `trace-id` element missing quotes around the namespace.
- Chunk length in characters instead of bytes (framing 1.1).
- `base64.decodestring` (removed in Python 3.9), default DSS private key type.
- `main()` reads global `args`; magic int return codes from `recv_chunk`.
- Duplicate `--start-time` / `--replay-start-time` destination.
- No host key verification and no way to enable it.
