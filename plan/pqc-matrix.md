# Post-quantum SSH support matrix (P7.1)

Status 2026-10-14. Server: `lib/confd_ssh` of this tree (OTP `ssh`), and OpenSSH 10.3. Client:
`ncrust` with `russh` 0.64.1 (`aws-lc-rs`).

## Key exchange

| Algorithm | ConfD/NSO server | OpenSSH 10.3 | ncrust (russh 0.64.1) | Default in ncrust |
|---|---|---|---|---|
| `mlkem768x25519-sha256` (hybrid ML-KEM-768 + X25519, RFC 9852) | yes; first in the default `/…/ssh/algorithms/kex`; needs an OTP `ssh` that implements it | yes (default) | yes | **yes, first** |
| `sntrup761x25519-sha512[@openssh.com]` | no (`upgrade_kex_alg` in `confdssh_algorithm.erl` does not know it) | yes | no | no |
| `curve25519-sha256`, `ecdh-sha2-nistp256/384/521`, `diffie-hellman-group14-sha256`, `-group16-sha512`, `-group-exchange-sha256` | yes | yes | yes | yes, after ML-KEM |

A server that lacks the ML-KEM exchange simply picks the next common name, so the default needs
no fallback code. `--ssh-kex mlkem768x25519-sha256` (only that name) against such a server fails
in the handshake with `SSH error: No common Kex algorithm - ours: [...], theirs: [...]`, before
authentication.

## Host key and user authentication

| Item | Server | ncrust | Note |
|---|---|---|---|
| PQ signature host key (ML-DSA, SLH-DSA) | none; defaults `ssh-ed25519,ssh-rsa,ecdsa-sha2-nistp256` | none | no SSH standard or library support yet |
| PQ signature user key | none | none | same |
| `ssh-ed25519`, `ecdsa-sha2-*`, `rsa-sha2-*`, `ssh-rsa` | yes | yes | |

## Crypto provider

`aws-lc-rs` (D1). No FIPS or CiscoSSL variant is built (P7.5): D1 says it is not required; the
`SshBackend` trait and the provider feature of `russh` keep it possible.
