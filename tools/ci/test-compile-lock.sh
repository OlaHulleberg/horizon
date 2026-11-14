#!/usr/bin/env bash
# Serialize the two large test compiles, preserving the configured compiler cache wrapper.
set -euo pipefail
crate_name= previous= test_unit=false
for arg in "$@"; do
  if [ "$previous" = --crate-name ]; then crate_name=$arg; fi
  if [ "$arg" = --test ]; then test_unit=true; fi
  previous=$arg
done
command=("$@")
if [ -n "${HORIZON_TEST_RUSTC_WRAPPER:-}" ]; then
  command=("$HORIZON_TEST_RUSTC_WRAPPER" "$@")
fi
if $test_unit && { [ "$crate_name" = horizon_client ] || [ "$crate_name" = horizon_client_lib ]; }; then
  exec 9> "${RUNNER_TEMP:?}/horizon-lib-test-compile.lock"
  flock -x 9
fi
exec "${command[@]}"
