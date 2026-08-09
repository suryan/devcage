#!/usr/bin/env bash
# Install/start a local container runtime for DevCage (macOS: Colima + Docker CLI).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

if ! command -v brew >/dev/null 2>&1; then
  echo "error: Homebrew required on macOS for this helper" >&2
  exit 1
fi

echo "==> brew install colima docker (if needed)"
brew list colima >/dev/null 2>&1 || brew install colima
brew list docker >/dev/null 2>&1 || brew install docker

echo "==> colima start"
if ! colima status 2>/dev/null | grep -qi running; then
  colima start
else
  echo "colima already running"
fi

# Ensure CLI talks to Colima's engine
docker context use colima >/dev/null 2>&1 || true
export DOCKER_HOST="unix://${HOME}/.colima/docker.sock"

echo "==> docker info"
docker info >/dev/null

echo "==> devcage doctor"
if [[ -x "${ROOT}/target/debug/devcage" ]]; then
  "${ROOT}/target/debug/devcage" doctor | head -40
elif command -v devcage >/dev/null 2>&1; then
  devcage doctor | head -40
else
  cargo run -q -- doctor | head -40
fi

echo "==> setup-runtime ok"
echo "    tip: export DOCKER_HOST=unix://\${HOME}/.colima/docker.sock"
echo "    or:  docker context use colima"
