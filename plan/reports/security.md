# Security review, 2026-10-13 (P5.7)

Scope: the `ncrust` crate at this commit, a client that sends what the user types to a NETCONF
server and prints what comes back. Items are checked against the tree, not against intentions.

## Checklist

- [x] **XML escaping.** Every text and attribute value that comes from the command line goes
  through `protocol::xmlw::XmlWriter`, which escapes `& < >` everywhere and `"`, CR, LF, TAB in
  attributes. Evidence: `tests/robustness.rs` `escaped_text_and_attributes_come_back_unchanged`
  and `a_hostile_xpath_stays_inside_its_attribute` (a million random cases each, hostile
  characters over-represented); `tests/golden_requests.rs` compares requests with Python's for
  quotes in xpath filters. `--db` must be an XML name
  (`Bad --db value`), timestamps and subscription ids are parsed, `--trace-id` is quoted.
- [x] **Deliberately unescaped input.** `--rpc-attribute` (inserted as attribute text) and the
  files of `--rpc`, `--edit-config`, `--edit-data`, `--copy-config`, session files and
  interactive input are sent as they are, as in Python; they are the user's own XML. They are
  streamed, so a hostile file cannot make the client allocate more than one buffer.
- [x] **No password in output.** The password is only read by the SSH back end. `Debug` of
  `TransportKind` and `AuthMethod` prints `<hidden>` (unit tests); nothing in `src/` prints a
  configuration with `{:?}`; `--debug` prints the pid and the negotiated algorithms only.
  Accepted, as with the Python script: a password given with `-p` is visible in the process list;
  a later change could read it from a file or the environment.
- [x] **Safe temporary files.** The program creates none. The optional external `xmllint`
  (`NETCONF_CONSOLE_XMLLINT=1`) is fed through a pipe and found through `PATH`, as in Python.
- [x] **Untrusted server data.** `tests/robustness.rs` feeds random and structured bytes to the
  framing decoders (both framings, any split), the hello parser, the built-in formatter, the
  schema extractor and the AAA filter: no panic, no output larger than the input, no allocation
  of an announced chunk size, same result for any split. `tests/fuzz_corpus/` keeps hand-made
  nasty inputs (billion laughs, DOCTYPE entities, size overflow, broken headers). The formatter
  rejects `<!DOCTYPE>`; `roxmltree` does not expand external entities and limits expansion.
- [x] **Memory safety.** `unsafe_code = "deny"` in `Cargo.toml`; there is no `unsafe` in `src/`.
- [x] **SSH defaults.** Any host key is accepted unless `--known-hosts` or `--host-key-fingerprint`
  is given (D4 in DECISIONS.md, same as Python). Unsupported algorithm names are rejected before any
  connection is made. Key files are read, never written; encrypted
  keys are refused with a message.
- [x] **Dependency audit.** `cargo deny check` (advisories, bans, licenses, sources) passes.
  `cargo audit` is not installed here; `cargo deny check advisories` uses the same RustSec database.

## Waiver

RUSTSEC-2023-0071 (Marvin attack, timing side channel in the `rsa` crate) is reported through
`russh` and `ssh-key`. No fixed version exists. It affects private-key operations with an RSA key,
here: signing for login with an RSA key file (`--privKeyFile`). An attacker would need to time many
logins at fine resolution; the tool is interactive and short-lived. The advisory is ignored in
`deny.toml` with this reason. Use Ed25519 or ECDSA keys, or revisit when `rsa` has a fix. RSA host
keys are only verified (public operation) and are not affected.

## Not covered

- A formal review of `russh` and `aws-lc-rs` themselves.
- Call home (`--listen`) accepts the first connection from anyone, as in Python.
