# Performance and robustness (P5.5, P5.6), 2026-10-13

## Performance

`tests/perf/bench.py` runs `netconf-console.py` and the `ncrust` release build against
`tests/perf/server.py` on loopback (Apple silicon, macOS, Python 3.14, output to `/dev/null`):

```
scenario                                     client     seconds   peak MiB
get-config, 100 MiB reply, plain             python         0.2        124
get-config, 100 MiB reply, plain             ncrust         0.1          3
get-config, 100 MiB reply, pretty            python         1.9       1035
get-config, 100 MiB reply, pretty            ncrust         1.5       2510
edit-config, 1000000 elements (51 MiB)       python         1.5         22
edit-config, 1000000 elements (51 MiB)       ncrust         1.5          3
get, --iter 10000, last                      python         1.2         22
get, --iter 10000, last                      ncrust         0.8          3
```

Thresholds, checked by running the script (not by a test, the numbers depend on the machine):

- Time: ncrust is not slower than Python in any scenario (equal where the server limits it).
- Streamed work stays bounded: plain output of a 100 MiB reply, a 1 M-element edit file and 10 000
  iterations stay at about 3 MiB, whatever the size.
- Formatted output (`pretty`, `all`, `last`, `noaaa`) needs the whole reply in memory. The built-in
  formatter uses about 25 times the size of the reply (2.5 GiB for 100 MiB); the Python script's
  pipe to `xmllint` peaks at about 10 times (1 GiB, the largest process of the tree). This is the
  one place where ncrust is worse.
  Replies of this size are not expected from NETCONF servers; `NETCONF_CONSOLE_XMLLINT=1` selects
  `xmllint` for such cases. A streaming formatter would remove the problem.

## Robustness

`tests/robustness.rs` (see `plan/reports/security.md` for what it covers) runs 2 000 random cases
per property in `cargo test`. `tools/fuzz_long.sh SECONDS` repeats it with 1 000 000 cases per
property and round, in release mode. Run on this tree: 3 rounds of 8 properties without a failure
(about 11 minutes), then the two escaping properties with 1 000 000 cases each. A run of the full
hour that the plan asks for has not been done yet: `tools/fuzz_long.sh 3600`.

Inputs worth keeping are in `tests/fuzz_corpus/` and are replayed by every test run
(`saved_inputs_are_handled`).
