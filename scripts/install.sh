#!/usr/bin/env bash
# Install the release binary into ${PREFIX}/bin (default: $HOME/.local).
# Override: PREFIX=/usr/local ./scripts/install.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

PREFIX="${PREFIX:-${HOME}/.local}"
BINDIR="${PREFIX}/bin"
mkdir -p "${BINDIR}"

echo "==> building release"
cargo build --release

SRC="${ROOT}/target/release/devcage"
DST="${BINDIR}/devcage"

if command -v install >/dev/null 2>&1; then
  install -m 755 "${SRC}" "${DST}"
else
  cp "${SRC}" "${DST}"
  chmod 755 "${DST}"
fi

echo "==> installed: ${DST}"
case ":${PATH}:" in
  *":${BINDIR}:"*) ;;
  *)
    echo "warn: ${BINDIR} is not on PATH; add it in your shell config (do not hardcode other machines' paths in the repo)"
    ;;
esac
