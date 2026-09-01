#!/bin/sh
set -eu

# Keep the measured floor honest while the project continues toward 100%.
# Override with ALTC_COVERAGE_MINIMUM=100 to run the aspirational gate.
minimum=${ALTC_COVERAGE_MINIMUM:-98}

cargo llvm-cov --workspace --all-targets --fail-under-lines "$minimum"
