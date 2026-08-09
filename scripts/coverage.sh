#!/usr/bin/env bash
# Run tests with coverage and fail if line coverage is below the gate.
# Override: DEVCAGE_COVERAGE_MIN=80 ./scripts/coverage.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

MIN="${DEVCAGE_COVERAGE_MIN:-80}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo not on PATH" >&2
  exit 1
fi

if ! cargo llvm-cov --version >/dev/null 2>&1; then
  echo "error: cargo-llvm-cov not installed" >&2
  echo "install: cargo install cargo-llvm-cov --locked" >&2
  echo "also:    rustup component add llvm-tools-preview" >&2
  exit 1
fi

echo "==> cargo llvm-cov (fail-under-lines=${MIN})"
mkdir -p target/coverage

cargo llvm-cov --workspace --all-targets \
  --fail-under-lines "${MIN}" \
  --lcov \
  --output-path target/coverage/lcov.info

cargo llvm-cov report --summary-only || true

echo "==> coverage ok (>= ${MIN}% lines)"
echo "    report: target/coverage/lcov.info"
