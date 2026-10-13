#!/bin/sh
# Long property-test run for tests/robustness.rs (P5.6). Stops after SECONDS (default 3600).
#
#   tools/fuzz_long.sh [SECONDS]
#
# A failing case is shrunk by proptest and saved to tests/robustness.proptest-regressions;
# add the input as a file to tests/fuzz_corpus/ and commit both.
set -eu
cd "$(dirname "$0")/.."
limit=${1:-3600}
end=$(( $(date +%s) + limit ))
rounds=0
while [ "$(date +%s)" -lt "$end" ]; do
    # A fresh random seed per round, many cases per round, optimised build for speed.
    PROPTEST_CASES=${PROPTEST_CASES:-200000} cargo test --release --test robustness -- --quiet
    rounds=$((rounds + 1))
done
echo "robustness: $rounds rounds of ${PROPTEST_CASES:-200000} cases per property, no failure"
