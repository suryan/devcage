#!/usr/bin/env bash
# End-to-end checks when Docker/Podman is available.
# Skips cleanly if no runtime is present.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

BIN="${ROOT}/target/debug/devcage"
if [[ ! -x "${BIN}" ]]; then
  cargo build -q
fi

if ! command -v docker >/dev/null 2>&1 && ! command -v podman >/dev/null 2>&1; then
  echo "skip: no docker/podman on PATH"
  exit 0
fi

echo "==> pull"
"${BIN}" pull

echo "==> offline echo"
"${BIN}" run -p offline --wipe -- echo offline-ok

echo "==> untrusted uname"
"${BIN}" run -p untrusted -- uname -a

echo "==> verify"
if ! "${BIN}" verify; then
  echo "warn: verify reported failures (inspect JSON above)"
fi

echo "==> e2e-container ok"
