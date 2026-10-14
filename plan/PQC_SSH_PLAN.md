# ncrust – Post-quantum SSH: Implementation Plan (P8–P16)

Extends [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md) (P0–P7). P7 enabled the one hybrid key
exchange that `russh` already has. This plan adds the rest of the algorithms below, plus a
feasibility track for the TLS counterparts (NETCONF over TLS, RFC 7589).

Conventions are those of the main plan: task IDs `P<phase>.<n>`, every task has a *Verify* line,
every phase has exit criteria, and every change passes `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings` and `cargo test`. Nothing outside
`src/transport/ssh/` may learn algorithm names (the architecture test enforces it).

## 1. Scope

Three specifications. Status read on 2026-10-14; the drafts change, so P8.1 pins the revision.

| Spec | Status | What it defines | OK to implement |
|---|---|---|---|
| [RFC 10042](https://www.rfc-editor.org/rfc/rfc10042) | Informational RFC, Aug 2026 | Hybrid PQ/T key exchange: `mlkem768x25519-sha256`, `mlkem768nistp256-sha256`, `mlkem1024nistp384-sha384` | SHOULD |
| [draft-harrison-sshm-mlkem-02](https://datatracker.ietf.org/doc/draft-harrison-sshm-mlkem/) | Individual draft, Standards Track, expires 28 Nov 2026 | Pure ML-KEM key exchange: `mlkem512-sha256`, `mlkem768-sha256`, `mlkem1024-sha384` | MAY (names not yet registered) |
| [draft-sfluhrer-ssh-mldsa-08](https://datatracker.ietf.org/doc/draft-sfluhrer-ssh-mldsa/) | Individual draft, Informational, expires 1 Mar 2027 | ML-DSA host and user keys: `ssh-mldsa-44`, `ssh-mldsa-65`, `ssh-mldsa-87` | SHOULD for `-65`, MAY for `-44`, `-87` |

Out of scope: SSHFP DNS records, SLH-DSA, `sntrup761x25519-sha512` (already tracked in
[pqc-matrix.md](pqc-matrix.md)), server-side SSH.

### Gap analysis (today: `russh` 0.64.1 with `aws-lc-rs`)

| Algorithm | In russh? | Work |
|---|---|---|
| `mlkem768x25519-sha256` | yes (default, first) | Verify against RFC 10042 text and against more peers |
| `mlkem768nistp256-sha256`, `mlkem1024nistp384-sha384` | no | New key exchange |
| `mlkem512-sha256`, `mlkem768-sha256`, `mlkem1024-sha384` | no | New key exchange |
| `ssh-mldsa-44/65/87` host key | no (`ssh-key` 0.7 has no ML-DSA algorithm) | New key type, signature check, known-hosts, fingerprint |
| `ssh-mldsa-44/65/87` user key | no | Key loading, signing, `server-sig-algs` handling |

### Why `russh` blocks us

Read from the `russh` 0.64.1 source on 2026-10-14. It is a quick read, not a full trace; P9.1
does the full trace and may find more touchpoints.

**Key exchange is a closed set.**
- `russh/src/kex/mod.rs` defines `KexAlgorithm` as an enum with fixed variants (DH groups,
  curve25519, `MlKem768X25519Kex`, and so on).
- The traits that each method implements (`KexType`, `KexAlgorithmImplementor`) are
  `pub(crate)`.
- Each name (for example `MLKEM768X25519_SHA256`) is a constant. A static map connects the
  name to its implementation.
- `Preferred.kex` is a list of `kex::Name` values, so it looks open. A name that the map does
  not know does nothing. We cannot write the missing implementation in ncrust, because the
  traits are private.
- Result: a new key exchange method can only be added inside `russh`.

**Host keys depend on `ssh-key` types.**
- When the key exchange ends, `russh` returns `server_host_key: Option<PublicKey>`. This is
  `ssh_key::PublicKey`.
- `ssh-key` 0.7.0-rc.11 has an `Algorithm` enum that, as far as we know, has no ML-DSA entry.
  **Not yet checked:** whether the enum can carry an unknown algorithm name. P9.4 and P9.5 find
  out.
- **Not yet traced:** signature verification and the `server-sig-algs` handling. They probably
  match on the known algorithms too.

**User keys have the same problem.** `load_secret_key` returns `ssh-key` private key types, and
public key authentication signs through them. ML-DSA user keys need a change in the same
places.

**Why P7 did not hit this.** `russh` already had `mlkem768x25519-sha256`. P7 only reordered a
list of names.

**What is not the problem.** The ncrust `SshBackend` trait hides the library well and does not
change. The obstacle is below it, inside `russh`. (`russh` also keeps its cipher and MAC traits
private, but this plan adds no cipher or MAC.)

P9 chooses the way out: upstream changes, a small patched fork, our own transport, or the
system OpenSSH.

## 2. Protocol facts the code must get right

Collected from the three texts so that reviewers can check the code against one page.

### 2.1 Hybrid key exchange (RFC 10042)

| Method | Classical part | PQ part | Hash | `C_INIT` = `C_PK2 ‖ C_PK1` | `S_REPLY` = `S_CT2 ‖ S_PK1` |
|---|---|---|---|---|---|
| `mlkem768x25519-sha256` | X25519 (RFC 8731) | ML-KEM-768 | SHA-256 | 1184 + 32 = 1216 | 1088 + 32 = 1120 |
| `mlkem768nistp256-sha256` | P-256 (RFC 5656) | ML-KEM-768 | SHA-256 | 1184 + 65 = 1249 | 1088 + 65 = 1153 |
| `mlkem1024nistp384-sha384` | P-384 (RFC 5656) | ML-KEM-1024 | SHA-384 | 1568 + 97 = 1665 | 1568 + 97 = 1665 |

(Lengths are for uncompressed EC points. The RFC says point compression "may be used"; see
Q2.)

- Messages: `SSH_MSG_KEX_HYBRID_INIT` = 30, `SSH_MSG_KEX_HYBRID_REPLY` = 31.
- The PQ part comes first in `C_INIT` and `S_REPLY`.
- `K = HASH(K_PQ ‖ K_CL)`. Both inputs are raw byte arrays, not mpints. `K_CL` is big-endian and
  fixed length: 32 bytes (X25519, P-256) or 48 bytes (P-384).
- `H` is computed as in RFC 4253, but `K` is encoded as an SSH **string**, not an mpint.
- The client MUST check that `S_REPLY` has exactly the expected length before decapsulating, and
  MUST disconnect with `SSH_DISCONNECT_KEY_EXCHANGE_FAILED` if the check or the decapsulation
  fails. The EC point and X25519 checks of RFC 5656 and RFC 8731 still apply.
- The `HASH` named by the method also drives the RFC 4253 section 7.2 key derivation.

### 2.2 Pure ML-KEM key exchange (draft-harrison-sshm-mlkem-02)

| Method | KEM | Hash | Public key / ciphertext |
|---|---|---|---|
| `mlkem512-sha256` | ML-KEM-512 | SHA-256 | 800 / 768 |
| `mlkem768-sha256` | ML-KEM-768 | SHA-256 | 1184 / 1088 |
| `mlkem1024-sha384` | ML-KEM-1024 | SHA-384 | 1568 / 1568 |

- Messages: `SSH_MSG_KEX_KEM_INIT` = 30 (`string C_INIT`), `SSH_MSG_KEX_KEM_REPLY` = 31
  (`string K_S`, `string S_REPLY`, `string signature`).
- **Different from the hybrid rule:** `K_PQ` is encoded as an **mpint** and goes straight into
  `H` and the key derivation. There is no extra hashing step.
- The client checks the ciphertext length and the three FIPS 203 section 7.3 checks, and
  disconnects with `KEY_EXCHANGE_FAILED` on failure.
- The draft is young (`-02`, text still has editorial gaps). Treat the encoding rules as
  provisional and re-check on every revision (P8.1). Names are requested from IANA, not yet
  registered.

### 2.3 ML-DSA keys (draft-sfluhrer-ssh-mldsa-08)

| Algorithm | Public key | Signature | FIPS 204 set |
|---|---|---|---|
| `ssh-mldsa-44` | 1312 | 2420 | ML-DSA-44 (category 2) |
| `ssh-mldsa-65` | 1952 | 3309 | ML-DSA-65 (category 3) |
| `ssh-mldsa-87` | 2592 | 4627 | ML-DSA-87 (category 5) |

- Public key blob: `string name`, `string key` (FIPS 204 algorithm 22 encoding).
- Signature blob: `string name`, `string signature` (FIPS 204 algorithm 2 output).
- Use the **pure** variant (not HashML-DSA) with an **empty context string**. Signing may be
  hedged or deterministic; both interoperate. Hedged is recommended.
- A verifier MUST reject a signature of the wrong length before running FIPS 204 algorithm 3.
- The signature algorithm name equals the key type name. There are no hash-variant names like
  `rsa-sha2-256`.
- Private key file format is out of scope of the draft. It warns that a key stored in expanded
  form cannot be converted to seed form, so tools that exchange keys need the seed.
- A client that loads a key must also see `ssh-mldsa-*` in the server's `server-sig-algs`
  extension (RFC 8308) before it offers the key in public key authentication.

### 2.4 Size and timing effects

The largest packets are the ML-KEM-1024 hybrid messages (about 1.7 KB) and an ML-DSA-87
signature (4627 bytes) plus host key (2592 bytes) in one reply. All stay below the 32768-byte
payload every SSH implementation must accept (RFC 4253 section 6.1). The effect on the
handshake is more round-trip bytes, not extra round trips; P15.3 measures it.

## 3. Phase overview

| Phase | Name | Result |
|---|---|---|
| P8 | Foundations | Spec digest pinned, conformance vectors, interop lab, crypto-source decision, TLS survey |
| P9 | Back-end route (gate) | A decided and prototyped way to add KEX and key types to the SSH stack |
| P10 | Hybrid ML-KEM key exchange | All three RFC 10042 methods work and interoperate |
| P11 | Pure ML-KEM key exchange | Three draft methods work, opt-in |
| P12 | ML-DSA host keys | `ssh-mldsa-*` host key verification, known-hosts, fingerprints |
| P13 | ML-DSA user keys | Public key authentication with ML-DSA keys |
| P14 | Policy, CLI and documentation | Defaults chosen, options and docs updated |
| P15 | Interop and hardening | Matrix against all peers, fuzzing, performance, CI |
| P16 | TLS counterparts | Findings and the TLS-side design for NETCONF over TLS |

Order: P8 → P9 → (P10, P11, P12 in parallel) → P13 (needs P12) → P14 → P15. P16 starts its
survey in P8 and needs no SSH result except the shared crypto choice (P8.6).

---

## P8 – Foundations

Goal: everything later phases test against exists, and no one codes from memory of a draft.

| ID | Task | Verify |
|---|---|---|
| P8.1 | Write `plan/pqc-spec-digest.md`: the tables of section 2 with section references, the exact draft revisions used, and a list of open drafting issues. Add a step to re-read the drafts at the start of P11 and P12. | Reviewed by a second person who checks it against the three texts. |
| P8.2 | Correct the existing docs: `pqc-matrix.md` and `pqc-extension-points.md` cite RFC 9852 for `mlkem768x25519-sha256`; the specification is RFC 10042. Refresh the matrix with the new gap analysis. | `rg 9852 plan/ src/` finds nothing; matrix lists all 9 new algorithm names. |
| P8.3 | Known-answer tests. Collect ML-KEM (FIPS 203) and ML-DSA (FIPS 204) vectors from NIST ACVP or the Wycheproof project. Build SSH-level vectors for the key exchange hash: take a transcript captured from two independent implementations and store it as a fixture (`tests/fixtures/pqc/`). | A test replays each transcript and reproduces `K`, `H`, the session id and the derived keys byte for byte. |
| P8.4 | Peer inventory. For each peer below, record version, build flags, and which of the nine algorithms it supports **as tested, not as documented**. Write `plan/reports/pqc-peers.md`. | The report has a row per peer and a command that reproduces each claim. |
| P8.5 | Interop lab: scripts in `tests/system/pqc/` that build or fetch each peer, start it with a NETCONF subsystem stub (as `openssh_kex.sh` does) and run ncrust against it. | `tests/system/pqc/run_all.sh` prints a pass/fail/unsupported table. |
| P8.6 | Crypto-source decision, recorded as **D11** in `DECISIONS.md`. Compare `aws-lc-rs` (already linked; check that its ML-KEM and ML-DSA APIs are stable or behind `unstable`), RustCrypto `ml-kem` (already in the tree through `russh`) and `ml-dsa`, and `libcrux`. Criteria: licence (`deny.toml`), MSRV 1.91, constant-time claims, audits, FIPS path (D1), hedged signing, seed-form key import, and whether the same crate serves rustls or OpenSSL for TLS (P16). | D11 written with the chosen crates, the rejected ones, and the reason. `cargo deny check` passes with the choice. |
| P8.7 | Time-boxed TLS survey (see P16.1); only the crypto-sharing question must be answered here. | One paragraph in D11 on what TLS needs from the same primitives. |

### Peers

Test every peer that can be built. The first two are the ones named by the requester.

| Peer | Why | Expected use |
|---|---|---|
| wolfSSH (wolfSSL) | Requested. Embedded server and client; wolfSSL has ML-KEM and ML-DSA. | Hybrid KEX, pure KEX, ML-DSA keys, if built with the right options |
| CycloneSSH (Oryx Embedded) | Requested. | Same |
| OpenSSH 10.x | Reference for hybrid KEX in the field; the local 10.3 is already used. | Hybrid KEX (check whether the two NIST-curve methods are present), ML-DSA unlikely |
| OQS-OpenSSH (Open Quantum Safe fork) | Has had PQ signature host keys for years, but with its own names. | ML-DSA only if it uses the draft names; otherwise record as incompatible |
| Go `golang.org/x/crypto/ssh` | Easy to script a server; has `mlkem768x25519-sha256`. | Hybrid KEX; a Go test server for the nistp variants if a patch exists |
| Bouncy Castle (Java), Apache MINA SSHD | Widely used in network gear. | Whatever they support |
| libssh, PuTTY, AsyncSSH, Dropbear | Cheap to add to the matrix. | Hybrid KEX mostly |
| Erlang/OTP `ssh` (ConfD, NSO) | The product this tool serves. | Hybrid KEX (checked in P7) |
| **ncrust test server** (`tests/common/ssh_server.rs`) | Needed when no peer has an algorithm. Built on the P9 stack. | Everything; a self-test cannot find a shared misreading, so it never replaces a real peer |

A peer that has only a draft-era or private name for an algorithm is **not** an interop
partner for that algorithm. Record it as such.

**Exit criteria P8:** spec digest merged, docs corrected, KATs and transcripts in tree, lab
runs, D11 written.

---

## P9 – Back-end route (gate)

Goal: pick, and prove on one small case, how new algorithms enter the SSH stack.

| Option | Idea | Cost | Risk |
|---|---|---|---|
| A | Upstream pull requests to `russh` (and `ssh-key`): public `KexType` registry, `Algorithm::Other` key types | Medium; depends on maintainers | Slow; the API is shaped by others |
| B | Maintain a patched `russh` through `[patch.crates-io]`, send the same changes upstream | Medium | Must follow upstream releases; supply-chain review of the fork |
| C | Write our own minimal SSH client transport (kex, host key, auth, channel, subsystem) | High | Security-sensitive code of our own |
| D | The `ssh-subprocess` backend with system OpenSSH | Low | Hybrid KEX only; no ML-DSA unless OpenSSH adds it; no call-home; no control of algorithm lists beyond `-o` |

Recommendation: **B and A together.** Keep the patch small, generic and upstreamable (a
registry for key exchange and key types, not ncrust-specific code). D stays as a fallback for
the key exchange only. C is not planned.

| ID | Task | Verify |
|---|---|---|
| P9.1 | Read how `russh` wires `kex::Name`, `Preferred`, `KexAlgorithm`, host key checks (`ssh_key::PublicKey`), `server-sig-algs`, and user-auth signing. List every place a new key exchange or key type touches. | `plan/reports/pqc-russh-touchpoints.md` with file and line references. |
| P9.2 | Prototype on the smallest case: add `mlkem768nistp256-sha256` through the chosen route, with the `SshBackend` trait unchanged. | `cargo test` shows the new name in `SshBackend::caps()` and a handshake against the in-process server. |
| P9.3 | Ask upstream: open an issue or draft pull request that proposes the extension points; record the answer and decide A or B. | **D12** in `DECISIONS.md` (route, owner, how patches are tracked, who reviews them). |
| P9.4 | Prove the host-key side with a throw-away key type that is not in `ssh-key` (for example a test-only algorithm). | A handshake verifies a signature by the new type; the architecture test still passes. |
| P9.5 | Decide how `ssh-key` types are avoided: if `PublicKey` cannot hold ML-DSA keys, introduce our own `HostKey` enum in `transport/ssh/hostkey.rs` and keep `ssh-key` only for classical types. | Design note in `pqc-extension-points.md`. |

**Exit criteria P9:** D12 written, prototype merged, extension-points doc updated. If neither A
nor B works, stop and ask for a decision on C or on shipping hybrid KEX through D only.

---

## P10 – Hybrid ML-KEM key exchange (RFC 10042)

| ID | Task | Verify |
|---|---|---|
| P10.1 | Audit the existing `mlkem768x25519-sha256` against section 2: length checks of `S_REPLY`, `K = HASH(K_PQ ‖ K_CL)` with fixed-length `K_CL`, `K` encoded as string in `H`, FIPS 203 checks, X25519 all-zero check, disconnect reason. | Table of rule → code location → test; failing rules become bugs with tests. |
| P10.2 | Implement `mlkem768nistp256-sha256`. Uncompressed P-256 point on send; validate peer point on curve (RFC 5656 section 4); `K_CL` as 32-byte big-endian. | Unit test with the P8.3 vector; handshake against the P8.5 peers that have it. |
| P10.3 | Implement `mlkem1024nistp384-sha384` (ML-KEM-1024, P-384, SHA-384, `K_CL` 48 bytes). | Same. |
| P10.4 | Negative tests for each method: short and long `S_REPLY`, bad EC point, invalid ML-KEM ciphertext (implicit rejection gives a wrong key, so the signature check fails), wrong signature, `K_CL` all zero. | Each case ends in `SSH_MSG_DISCONNECT` code 3 (`KEY_EXCHANGE_FAILED`) or a signature error, never a panic or a hang. |
| P10.5 | Rekey: the new methods work in a second key exchange on a live session. | Test with a rekey after N bytes against the in-process server and one real peer. |
| P10.6 | Property tests: random transcripts, `K` equal on both sides, keys differ between sessions. | `proptest` passes at 10 000 cases. |

**Exit criteria P10:** all three methods negotiate with at least two independent peers each,
where peers exist (P8.4); a method with fewer peers is listed with the reason.

---

## P11 – Pure ML-KEM key exchange (draft)

Opt-in only: the draft says MAY, the names are not registered, and the wire encoding may still
change. Keep the code behind the Cargo feature `pq-draft-kex` (off by default) until the draft
is stable.

| ID | Task | Verify |
|---|---|---|
| P11.1 | Re-read the draft (P8.1); note any change since `-02`. | Digest updated, diff listed in the PR. |
| P11.2 | Implement `SSH_MSG_KEX_KEM_INIT/REPLY` and the three methods. `K_PQ` as mpint, ML-KEM-512/768/1024, SHA-256/256/384. | KAT from P8.3 reproduces `H`; handshake with the in-process server. |
| P11.3 | Negative tests as in P10.4 (wrong ciphertext length, failed FIPS 203 checks). | Same disconnect rule. |
| P11.4 | Interop with every peer that implements the draft names. If none does, state so in `pqc-peers.md` and keep the feature off. | Report row per peer. |
| P11.5 | Document that the leading zero byte rule for mpint matters: a `K_PQ` whose first bit is set needs a `0x00` prefix; one with leading zero bytes loses them. | Unit tests with both cases, including a shared secret that starts with `0x00`. |

**Exit criteria P11:** tests green under the feature; report says which peers interoperate.

---

## P12 – ML-DSA host keys

| ID | Task | Verify |
|---|---|---|
| P12.1 | Public key codec for the three key types (section 2.3): parse and encode the blob, check the key length per type. | Unit tests with FIPS 204 vectors; a wrong length gives a clear error. |
| P12.2 | Signature verify: check length first, then FIPS 204 algorithm 3, pure mode, empty context. | KAT vectors pass; a flipped bit anywhere fails; wrong-length input fails before the crypto call. |
| P12.3 | Wire into the host key algorithm list (`host_key_algorithms()`), so `--ssh-host-key-algs ssh-mldsa-65` works, including the OpenSSH list syntax (`+`, `-`, `^`, patterns). | `ssh_e2e` test negotiates `ssh-mldsa-65` with the in-process server; `--debug` shows it. |
| P12.4 | Host key checks: `--known-hosts` must read and write lines with `ssh-mldsa-*`; `--host-key-fingerprint SHA256:…` hashes the encoded blob (RFC 4253 section 6.6 form). | Test: a known_hosts line with an ML-DSA key matches; a changed key is refused. |
| P12.5 | The negotiated name and the key type in the reply must agree; a server that signs with another type is refused. | Negative test. |
| P12.6 | Interop: one real server with an ML-DSA host key, if any exists (wolfSSH, CycloneSSH or others). | Report row; otherwise a recorded gap and the in-process test as the only evidence (never silently "pass"). |

**Exit criteria P12:** host key verification works for all three sizes in the in-process test
and with every real peer that offers one.

---

## P13 – ML-DSA user keys

| ID | Task | Verify |
|---|---|---|
| P13.1 | Private key loading for `--privKeyFile`. Decide the formats: the OpenSSH container (`openssh-key-v1`) with key type `ssh-mldsa-*` is the likely one, but the draft leaves the private encoding open. Record what wolfSSH, CycloneSSH and OpenSSH-based tools write (P8.4) and accept those. Accept seed form; accept expanded form only when the peer tools need it. | Test keys written by each tool load; a sign/verify round trip works. |
| P13.2 | Passphrase-protected keys, the same way as for classical keys. | Test with an encrypted key. |
| P13.3 | Signing: pure ML-DSA, empty context, hedged by default (random from the OS RNG). The signed data is the RFC 4252 section 7 blob; the signature algorithm name equals the key type. | Server in-process verifies; deterministic mode available in tests for reproducible vectors. |
| P13.4 | `server-sig-algs` (RFC 8308): offer an ML-DSA key only if the server lists its name; otherwise skip it and try the next key, with a `--debug` note. Also support `publickey-hostbound@openssh.com` as the classical keys do. | Tests: server lists it → used; server does not → skipped. |
| P13.5 | `--privKeyType` hint accepts the three names (`keys.rs`, `type_hint_warning`). | Unit test. |
| P13.6 | Keep key material out of memory dumps and logs: zero secrets on drop; `Debug` never prints them. | Test on `Debug` output; review note. |
| P13.7 | Test key generator for the test suite and the lab (`tools/` or a `dev-dependencies` helper), since stock `ssh-keygen` may not create ML-DSA keys. Not shipped in the binary. | Generator output loads in ncrust and in at least one other implementation. |
| P13.8 | Interop with real servers that accept ML-DSA user keys. | Report row per peer. |

**Exit criteria P13:** NETCONF session over SSH, authenticated by an ML-DSA user key, against
the in-process server and every real peer that supports it.

---

## P14 – Policy, CLI and documentation

| ID | Task | Verify |
|---|---|---|
| P14.1 | Decide the default order and record it as **D13**. Proposal: key exchange `mlkem768x25519-sha256`, `mlkem1024nistp384-sha384`, `mlkem768nistp256-sha256`, then the classical list. Pure ML-KEM never in the defaults. Host keys: ML-DSA names are **supported** from P12 and **offered last** at first; move `ssh-mldsa-65` forward only after P15 shows no interop trouble. | D13 written with the reason for each placement. |
| P14.2 | `--ssh-kex` and `--ssh-host-key-algs` need no new syntax; check the list rules with the new names, and that `--ssh-kex 'mlkem*'` selects all PQ methods. | Test. |
| P14.3 | Optional: a "require PQ" policy. If the plain `--ssh-kex` list does not give a clear message, add `--ssh-require-pq`. Only add it if a user needs it; no flag by default. | Decision recorded; if added, option-map updated and tested. |
| P14.4 | `--debug` output: negotiated kex and host key already printed; add the ML-KEM variant and the signature algorithm for the user key. | Golden test. |
| P14.5 | Update README (supported algorithms table), `pqc-matrix.md`, `pqc-extension-points.md`, `option-map.md`, `DEVIATIONS.md`. Write the pages with the `simple-english` rules. | Reviewed; links work. |
| P14.6 | Error messages: a server that offers only unsupported names gets a list of both sides' names (as today); a draft-only feature that is compiled out says which Cargo feature enables it. | Test per message. |

---

## P15 – Interop and hardening

| ID | Task | Verify |
|---|---|---|
| P15.1 | Full matrix: every ncrust algorithm against every peer, both directions of preference (client-first and server-first lists). Save as `plan/reports/pqc-interop-2.md`. | Table has no blank cells: pass, fail with issue link, or unsupported by peer. |
| P15.2 | Fuzzing: `cargo fuzz` targets for the KEX reply parser, host key blob parser, signature parser, private key parser. | 1 hour per target without crash; corpus saved. |
| P15.3 | Performance: handshake time and bytes per method against the classical baseline, with a loopback peer and with 50 ms simulated delay. Save in `plan/reports/performance.md`. | Report; no method adds more than one round trip. |
| P15.4 | Constant-time and memory review of the chosen crates (D11); note findings in `plan/reports/security.md`. | Checklist signed off. |
| P15.5 | CI job: run the in-process matrix always; run the lab (P8.5) nightly where the peers can be built. | Job green; artefacts stored. |
| P15.6 | Re-check drafts and RFC errata; bump revisions in the digest; check whether IANA registered the draft names (they may differ). | Digest updated with date. |

**Exit criteria P15:** matrix complete, no open crash, CI in place.

---

## P16 – TLS counterparts (NETCONF over TLS)

Expectation from the requester: TLS support for these algorithms is more likely to exist in the
Rust ecosystem than SSH support was. The plan is to confirm that, not to assume it. ncrust has
no TLS transport today; `Session` and `Transport` are already independent of the carrier
(PLAN.md section 6), so TLS is another `Transport`.

### Counterparts

| SSH item | TLS 1.3 counterpart | Specification to check |
|---|---|---|
| `mlkem768x25519-sha256` | `X25519MLKEM768` named group | draft-ietf-tls-ecdhe-mlkem |
| `mlkem768nistp256-sha256` | `SecP256r1MLKEM768` | same |
| `mlkem1024nistp384-sha384` | `SecP384r1MLKEM1024` | same |
| `mlkem512/768/1024-*` | `MLKEM512/768/1024` pure groups | draft-ietf-tls-mlkem |
| `ssh-mldsa-44/65/87` | `mldsa44/65/87` signature schemes; ML-DSA keys in X.509 certificates | draft-ietf-tls-mldsa; RFC 9881 (X.509) |
| SSH carrier | NETCONF over TLS | RFC 7589 and its TLS 1.3 update (check current status) |

IANA code points and draft names change; the survey records the values it actually verified.

| ID | Task | Verify |
|---|---|---|
| P16.1 | Survey, with versions from `Cargo.lock` (today `rustls` 0.23.45 appears in the lock file): does `rustls` with the `aws-lc-rs` provider offer each named group? Does `rustls-webpki` verify ML-DSA certificates and the TLS ML-DSA signature schemes? Same questions for OpenSSL 3.5 or later (`openssl` crate) and wolfSSL (`wolfssl` bindings). Write `plan/reports/pqc-tls-survey.md`. | Table: library × feature, each cell backed by a running example (a `ncrust`-independent probe program against `openssl s_server` or `wolfssl` server). |
| P16.2 | Decide the TLS library; **D14** in `DECISIONS.md`. Criteria as D11, plus: client certificates (RFC 7589 needs mutual authentication), fingerprint-based peer checks, and a build that does not drag OpenSSL into the default binary. | D14 written. |
| P16.3 | If the library supports the hybrid groups: add a `tls` Cargo feature (off by default) with a `Transport` for NETCONF over TLS (RFC 7589 framing is the same as over SSH: hello, then chunked frames), reusing `Session`. Options: `--proto tls`, `--tls-cert`, `--tls-key`, `--tls-ca`, `--tls-groups`. | Mock-server end-to-end test; `architecture.rs` extended so only `transport/tls/` knows the TLS library. |
| P16.4 | Group and signature scheme policy using the same list syntax as `--ssh-kex` (`--tls-groups`), same default philosophy as D13. | Tests like P14.2. |
| P16.5 | Interop: `openssl s_server` 3.5+, wolfSSL server, and a NETCONF-over-TLS capable server (for example `netopeer2`) for the hybrid groups, and for ML-DSA certificates if P16.1 found support. | Report `plan/reports/pqc-tls-interop.md`. |
| P16.6 | If a TLS feature is missing (likely ML-DSA certificates and the pure ML-KEM groups): record it with the tracking issue and the version that is expected to add it; do not build our own TLS. | The survey lists gaps with links. |

P16.1 and P16.2 may run in parallel with P8. P16.3 onwards waits for P14, so that the two
transports use the same policy model.

---

## 4. Open questions

| # | Question | Decide in |
|---|---|---|
| Q1 | Route A or B for the SSH stack (section P9)? | P9.3 |
| Q2 | RFC 10042 says compressed EC points "may be used". Do peers send them? Our length check must match what peers send. Accept both forms on receive, send uncompressed. | P10.2, with peer data from P8.4 |
| Q3 | Draft private key format for ML-DSA: seed, expanded, or OpenSSH container with which layout? | P13.1 |
| Q4 | When to put `ssh-mldsa-65` before the classical host keys by default? | P14.1 / P15 |
| Q5 | Do the draft names remain `mlkem512-sha256` etc. after IANA review? | P15.6 |
| Q6 | A FIPS provider (D1)? ML-DSA and ML-KEM live in FIPS-validated modules only for some versions of `aws-lc`; revisit if D1 changes. | P8.6 |

## 5. Risks

| Risk | Phase | Mitigation |
|---|---|---|
| `russh` internals stay private; upstream is slow | P9–P13 | Route B; keep patches small; D fallback for key exchange only |
| Drafts change encodings after we implement | P11, P12 | Digest with revisions (P8.1); pure KEX behind a feature; re-read at every phase start (P15.6) |
| Two implementations share the same misreading | all | Transcripts from independent peers (P8.3); no pass counted from self-test alone |
| Few or no real peers for ML-DSA | P12, P13 | Record the gap openly; test with several peers as they appear; keep ML-DSA out of the defaults until proven |
| New `unsafe` or unaudited crypto crate | P8.6 | D11 review; `unsafe_code = "deny"` stays; `cargo deny` |
| Large messages break an old server | P15 | Section 2.4 limits; interop matrix includes OTP `ssh` (ConfD) and embedded stacks |
| Hybrid KEX breaks `--known-hosts` or logging because of unknown algorithm names | P12, P14 | Tests in P12.4 and P14.4 |
| TLS libraries lack ML-DSA | P16 | Findings only; no home-made TLS |

## 6. Definition of done for this plan

1. All nine SSH algorithm names negotiate in the in-process matrix (pure ML-KEM only with its
   feature) and, where a real peer supports them, with that peer.
2. A NETCONF session over SSH runs with `mlkem*` key exchange, an ML-DSA host key and an ML-DSA
   user key.
3. The defaults are recorded (D13) and the README table matches the code.
4. `pqc-peers.md`, `pqc-interop-2.md` and `pqc-tls-survey.md` exist and are current.
5. The TLS decision (D14) is made, and a transport exists if the libraries allow it.
6. No code outside `transport/ssh/` (and `transport/tls/`) knows an algorithm or library name.
