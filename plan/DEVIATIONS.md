# Intentional deviations from netconf-console.py

Where ncrust behaves differently from the Python script on purpose. Add an entry whenever a test or a fix changes observable behaviour.

| Area | Python | ncrust | Why |
|---|---|---|---|
| `--proto` other than `ssh`/`tcp` | silently uses tcp | error `Bad --proto value: X` | typo protection |
| `-v` other than `1.0`/`1.1` | advertises no base capability | error `Bad --version value: X` | the session cannot work |
| Unknown `-s` style | prints replies unformatted | error `Bad --outputStyle value: X` | typo protection |
| `-s plain` with `--iter N` (N > 1) | fails after the first reply (stdout is closed) | prints the first reply, skips the rest | what the Python help text documents |
| `-x` xpath | value starts with a space (words joined with a leading space) | no leading space | cleaner request |
| XML special characters in xpath, `--db`, ids | inserted unescaped (invalid XML for e.g. `<` in an xpath, or both quote kinds) | escaped; `--db` must be an XML name (`Bad --db value: X`) | correctness / no injection |
| Connection failure text | `Failed to connect to H: [Errno 61] Connection refused` | `Failed to connect to H: Connection refused (os error 61)` | OS error text comes from Rust's `io::Error` |
| Unresolvable host | uncaught exception | same `Failed to connect to H: ...` message | consistent error |
| Server hello | capabilities must be in the first child of `<hello>` | `<capabilities>` is found anywhere below `<hello>` | `session-id` may come first |
| Chunk size in 1.1 framing | counts characters | counts bytes | RFC 6242 |
| Socket write errors | printed as `socket error: ...`, run continues | reported as an error, exit code 1 | hiding failures is not useful |
| Missing/unreadable input file | traceback | `FILE: <error>` on stderr, exit code 1 | readable error |
| Usage errors | optparse text, exit code 2 | clap text, exit code 2 | different parser |
