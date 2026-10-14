# Post-quantum extension points (P3.10)

Everything cryptographic lives in `src/transport/ssh/`. Nothing outside that directory knows
algorithm names, key types or the SSH library (the architecture test enforces this). P7 relies on
the places below.

## Where to change what

| Goal | Where | What to do |
|---|---|---|
| Change the default key exchange order | `russh_backend.rs` (`supported()`) | The defaults are the library's list (`Preferred::DEFAULT.kex`), in which `mlkem768x25519-sha256` is first (P7.2, [pqc-matrix.md](pqc-matrix.md)). To drop or reorder one, filter or sort there. |
| Support another PQ key exchange name | the back end | `supported()` takes the names from the library, so a library upgrade is enough; `--ssh-kex` accepts the new name from then on and the default list follows the library's order. |
| Add a host key algorithm (PQ signatures) | `russh_backend.rs` (`host_key_algorithms()`) | The list comes from the library's `Preferred::DEFAULT.key`. A new algorithm appears there when the library supports it; `--ssh-host-key-algs` accepts it by name from then on. |
| Add a user key type (PQ signatures) | `keys.rs` (`type_hint_warning`) and `russh_backend.rs` (`open`) | `--privKeyType` is only a hint; add the new type name to `type_hint_warning`. Loading is done by the library (`load_secret_key`), so the key file must be a format it can read. |
| Select another crypto provider (FIPS, CiscoSSL) | `Cargo.toml` features, `russh_backend.rs` | The `russh` dependency selects `aws-lc-rs`. A provider variant is a different feature of the same dependency; the `SshBackend` trait does not change (P7.5). |
| Use another SSH library | new `*_backend.rs` | Implement `SshBackend` (`caps()` and `connect()`), select it in `ssh::default_backend()`. |

## How names flow

1. `cli::validate` copies `--ssh-kex`, `--ssh-host-key-algs`, `--ssh-ciphers`, `--ssh-macs` as
   strings into `config::AlgorithmSpecs`. No name is checked there.
2. `ssh::Prepared::new` asks the back end what it supports (`SshBackend::caps()`) and calls
   `algorithms::resolve_all`. It applies the OpenSSH list syntax (`+`, `-`, `^`, `*`/`?`
   patterns) and reports unknown names with the supported ones, before any connection is made.
3. The back end receives the resolved lists (`SshOptions::algorithms`) and turns them into the
   library's types. A name the library cannot map is an error, never silently dropped.
4. After the key exchange the back end reports what was agreed (`Negotiated`); `--debug` prints
   it on stderr (`SSH algorithms: kex=... host-key=... cipher=... mac=...`).

## What is tested

- `tests/ssh_e2e.rs` negotiates with in-process servers: the PQ exchange is the default when the
  server has it, the classical one is used when it does not, and requiring the PQ exchange
  against a classical-only server fails in the handshake. An unsupported name fails before
  connecting.
- `tests/system/openssh_kex.sh` negotiates with the installed OpenSSH; results with it and with
  ConfD are in [reports/pqc-interop.md](reports/pqc-interop.md).
