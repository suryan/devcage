#!/usr/bin/env bash
# Full quality gate: format, clippy (pedantic), script lint, tests, coverage.
# Used by CI and pre-commit. From repository root:
#   ./scripts/check.sh
#   ./scripts/check.sh --quick   # skip coverage (local iteration only)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

QUICK=0
for arg in "$@"; do
  case "${arg}" in
    --quick|-q) QUICK=1 ;;
    -h|--help)
      echo "Usage: scripts/check.sh [--quick]"
      echo "  --quick  skip coverage (fmt/clippy/scripts/tests only)"
      exit 0
      ;;
    *)
      echo "error: unknown arg: ${arg}" >&2
      exit 2
      ;;
  esac
done

echo "==> cargo fmt --check"
cargo fmt --all -- --check

echo "==> cargo clippy (deny warnings; pedantic/nursery via Cargo.toml)"
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "==> script lint"
"${ROOT}/scripts/lint-scripts.sh"

echo "==> cargo test"
cargo test --workspace --all-targets

if [[ "${QUICK}" -eq 0 ]]; then
  "${ROOT}/scripts/coverage.sh"
else
  echo "==> coverage skipped (--quick)"
fi

echo "==> check ok"
