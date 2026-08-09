#!/usr/bin/env bash
# Pre-commit gate: formatting, clippy, shell scripts, tests, coverage.
# Invoked by the pre-commit framework or .git/hooks/pre-commit.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

echo "==> devcage pre-commit"
exec "${ROOT}/scripts/check.sh"
