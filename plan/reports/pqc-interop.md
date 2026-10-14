# PQ key exchange interop (P7.2, P7.3, P7.6)

Run 2026-10-14 with `ncrust --debug --hello`, which prints the negotiated algorithms on stderr.

## ConfD of the trunk dev tree (example `1-2-3-start-query-model`, port 2022)

| Command line | Negotiated kex |
|---|---|
| default | `mlkem768x25519-sha256` |
| `--ssh-kex curve25519-sha256` | `curve25519-sha256` |
| `--ssh-kex curve25519-sha256,mlkem768x25519-sha256` | `curve25519-sha256` (given order wins) |

## OpenSSH 10.3 (`tests/system/openssh_kex.sh`)

| Command line | Negotiated kex |
|---|---|
| default | `mlkem768x25519-sha256` |
| `--ssh-kex curve25519-sha256` | `curve25519-sha256` |
| `--ssh-kex mlkem768x25519-sha256` | `mlkem768x25519-sha256` |

## In-process servers (`tests/ssh_e2e.rs`)

| Server offers | Command line | Result |
|---|---|---|
| ML-KEM and curve25519 | default | ML-KEM |
| only curve25519 | default | curve25519 (fallback) |
| only ML-KEM | default | ML-KEM |
| only curve25519 | `--ssh-kex mlkem768x25519-sha256` | exit 1, `No common Kex algorithm` |
| only ML-KEM | `--ssh-kex curve25519-sha256` | exit 1, `SSH error` |

NSO itself was not run; it uses the same `confd_ssh` application as ConfD. A CI job against
OpenSSH is not added: the script above needs an `sshd` binary and runs on the developer machine.
