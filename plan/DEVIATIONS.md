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
| `-s plain` with several replies (`--iter N`, session files, interactive) | fails after the first reply (stdout is closed) | prints the first reply, skips the rest | what the Python help text documents |
| `--trace-id` | element written as `<trace-id xmlns=http://...>` (invalid XML) | quoted namespace | correctness |
| Message ids of repeated non-`get` requests (`--iter`) | always 1 | 1, 2, 3, ... | unique ids |
| `--config-filter` other than `true`/`false` | silently ignored | error `Bad --config-filter value: X` | typo protection |
| `--start-time`, `--stop-time`, `--replay-start-time` | not checked (`--stop-time` crashes with a traceback if malformed) | must be `YYYY-MM-DDTHH:MM:SS`, else `Bad --stop-time value: X` | readable error |
| More than one positional argument | all ignored | usage error | typo protection |
| `replayComplete` detection (with `--stop-time`) | regular expression that fails if the notification contains a line break between the tags | XML parsing | robustness |
| Interactive prompts | printed late when stdout is not a terminal (Python buffers them, `xmllint` output comes first) | printed before the reply they belong to | no child process |
| Output flushing | flushed after each reply (closing stdout) | flushed after each reply and each raw read | same intent |
| `--hello` etc. with `-s noaaa` or `pretty` | needs `xmllint` and `awk` in `PATH` | built in; `NETCONF_CONSOLE_XMLLINT=1` still uses `xmllint` | fewer dependencies |

## Known gaps of the built-in formatter (`-s pretty|all|last|noaaa`)

It reproduces libxml2 2.9.13 `xmllint --format` (verified with fixtures and randomized comparison). Not covered:

- Documents with a `<!DOCTYPE>` are rejected (`xmllint failed. Is the returned XML ok?`).
- Declared encodings other than UTF-8 are not converted.
- Raw carriage returns: libxml2 splits text at CR LF, which can change which whitespace is dropped. NETCONF servers do not send them.
- Names and namespaces are not validated beyond well-formedness, as libxml2 only warns about namespace errors.
- Other libxml2 versions may differ in rare cases; use `NETCONF_CONSOLE_XMLLINT=1` to get the installed `xmllint`.
