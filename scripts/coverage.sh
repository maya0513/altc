#!/bin/sh
set -eu

minimum=${ALTC_COVERAGE_MINIMUM:-98}
exec cargo xtask coverage --minimum "$minimum"
