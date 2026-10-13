# Decisions

One entry per decision: what, when, why. Answers to the open questions of PLAN.md §10 (P0.1).

## D1 – Crypto provider / FIPS (2026-10-13)
- **Decision:** Use a Rust crypto provider (russh with `aws-lc-rs`, selectable by cargo feature). CiscoSSL/FIPS is not required now.
- **Why:** Keeps the build self-contained. `SshBackend` and the provider feature keep a later CiscoSSL/FIPS variant possible (P7.5).
- **Revisit:** when a FIPS requirement for the tool is raised.

## D2 – PQ roadmap of the server (2026-10-13)
- **Decision:** Server-side PQ support is unknown. Algorithms stay configurable by name; PQ defaults are enabled only after P7.1 produces the support matrix.
- **Why:** Avoids guessing; the design does not depend on the answer.

## D3 – Output compatibility (2026-10-13)
- **Decision:** Pretty-printing must be byte-identical to `xmllint --format`.
- **Why:** Many lux tests compare formatted output. Verified by the P2.7 differential corpus.

## D4 – SSH host key default (2026-10-13)
- **Decision:** Accept any host key by default (as the Python script does); verification is opt-in (`--known-hosts`, `--host-key-fingerprint`).
- **Why:** Behaviour compatibility with existing tests and demos.

## D5 – Target platforms (2026-10-13)
- **Decision:** Linux x86_64 and aarch64, macOS arm64 and x86_64.

## D6 – CI system (2026-10-13)
- **Decision:** GitHub Actions for the `ncrust` repository.

## D7 – Toolchain (2026-10-13)
- **Decision:** Rust 1.91.1 pinned in `rust-toolchain.toml`, edition 2024; package `ncrust`, binary `netconf-console`.
- **Why:** Matches the toolchain already installed on the development machine.
