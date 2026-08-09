#!/usr/bin/env bash
# Build DevCage from the repository root (detected from this script's location).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

RELEASE=0
for arg in "$@"; do
  case "${arg}" in
    --release|-r) RELEASE=1 ;;
    -h|--help)
      echo "Usage: scripts/build.sh [--release]"
      exit 0
      ;;
    *)
      echo "error: unknown arg: ${arg}" >&2
      exit 2
      ;;
  esac
done

if [[ "${RELEASE}" -eq 1 ]]; then
  echo "==> cargo build --release"
  cargo build --release
  echo "==> binary: ${ROOT}/target/release/devcage"
else
  echo "==> cargo build"
  cargo build
  echo "==> binary: ${ROOT}/target/debug/devcage"
fi
