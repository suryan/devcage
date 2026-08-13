#!/usr/bin/env bash
# Install DevCage from source (macOS + Linux).
#
# Builds and installs a release binary. No GitHub release artifacts required —
# only git, a C toolchain, and Rust (rustup is bootstrapped if cargo is missing).
#
# One-liner (from GitHub):
#   curl -fsSL https://raw.githubusercontent.com/suryan/devcage/main/scripts/install.sh \
#     | bash -s -- --yes
#
# From a local clone (auto-detected when this file lives in a checkout):
#   ./scripts/install.sh
#   PREFIX=/usr/local ./scripts/install.sh
#
# Environment (optional):
#   DEVCAGE_REPO    git URL (default: https://github.com/suryan/devcage.git)
#   DEVCAGE_REF     branch/tag/commit (default: main)
#   DEVCAGE_PREFIX  install prefix (default: $HOME/.local) → bin/devcage
#   DEVCAGE_DIR     source checkout dir (default: $HOME/.local/src/devcage)
#   PREFIX          alias for DEVCAGE_PREFIX (compat with older docs)
#   CARGO_HOME / RUSTUP_HOME  standard rustup vars
#
# Exit codes: 0 success, 1 user/env error, 2 build/install failure

set -euo pipefail

REPO_DEFAULT="https://github.com/suryan/devcage.git"
REPO="${DEVCAGE_REPO:-${REPO_DEFAULT}}"
REF="${DEVCAGE_REF:-main}"
PREFIX="${DEVCAGE_PREFIX:-${PREFIX:-${HOME}/.local}}"
SRC_DIR="${DEVCAGE_DIR:-${HOME}/.local/src/devcage}"
YES=0
KEEP_SRC=1
LOCAL_ONLY=0
LOCAL_FORCED=0
IN_TREE=0

usage() {
  cat <<'EOF'
Usage: install.sh [options]

  --prefix DIR     Install prefix (binary → DIR/bin/devcage). Default: ~/.local
  --ref REF        Git branch, tag, or commit. Default: main
  --repo URL       Git remote URL
  --dir DIR        Source checkout directory. Default: ~/.local/src/devcage
  --no-keep-src    Remove the checkout after a successful remote install
  --local          Build the repo containing this script (no clone/fetch)
  --yes, -y        Non-interactive (auto-install rustup if needed)
  -h, --help       Show this help

Examples:
  # Recommended first-time setup
  curl -fsSL https://raw.githubusercontent.com/suryan/devcage/main/scripts/install.sh \
    | bash -s -- --yes

  ./scripts/install.sh
  ./scripts/install.sh --local
  DEVCAGE_REF=v0.7.0 ./scripts/install.sh --yes
EOF
}

log() { printf '==> %s\n' "$*"; }
warn() { printf 'warn: %s\n' "$*" >&2; }
die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

# Set IN_TREE=1 when this script is a real file inside a DevCage checkout.
# Always returns 0 so callers can invoke it under `set -e` (not as an `if` test).
detect_in_tree() {
  IN_TREE=0
  local src dir root
  src="${BASH_SOURCE[0]:-}"
  if [[ -z "${src}" || ! -f "${src}" ]]; then
    return 0
  fi
  dir="$(cd "$(dirname "${src}")" && pwd)"
  root="$(cd "${dir}/.." && pwd)"
  if [[ -f "${root}/Cargo.toml" ]] && grep -q '^name = "devcage"$' "${root}/Cargo.toml"; then
    IN_TREE=1
  fi
  return 0
}

tool_semver() {
  # "rustc 1.93.1 (…)" / "cargo 1.93.1 (…)" → 1.93.1
  local line="$1"
  local rest="${line#* }"
  printf '%s' "${rest%% *}"
}

# ─── args ────────────────────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix)
      PREFIX="${2:-}"
      shift 2
      ;;
    --prefix=*)
      PREFIX="${1#*=}"
      shift
      ;;
    --ref)
      REF="${2:-}"
      shift 2
      ;;
    --ref=*)
      REF="${1#*=}"
      shift
      ;;
    --repo)
      REPO="${2:-}"
      shift 2
      ;;
    --repo=*)
      REPO="${1#*=}"
      shift
      ;;
    --dir)
      SRC_DIR="${2:-}"
      shift 2
      ;;
    --dir=*)
      SRC_DIR="${1#*=}"
      shift
      ;;
    --no-keep-src)
      KEEP_SRC=0
      shift
      ;;
    --local)
      LOCAL_ONLY=1
      LOCAL_FORCED=1
      shift
      ;;
    --yes | -y)
      YES=1
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      die "unknown option: $1 (try --help)"
      ;;
  esac
done

if [[ -z "${PREFIX}" ]]; then
  die "--prefix must not be empty"
fi

BINDIR="${PREFIX}/bin"
BIN="${BINDIR}/devcage"

detect_in_tree
# Piped curl|bash has no checkout; --local is only valid from a real file.
if [[ "${LOCAL_FORCED}" -eq 1 && "${IN_TREE}" -ne 1 ]]; then
  die "--local requires running this script from a DevCage clone (scripts/install.sh)"
fi
if [[ "${LOCAL_FORCED}" -eq 0 && "${IN_TREE}" -eq 1 ]]; then
  LOCAL_ONLY=1
fi

# ─── platform ────────────────────────────────────────────────────────────────

OS="$(uname -s 2>/dev/null || echo unknown)"
ARCH="$(uname -m 2>/dev/null || echo unknown)"
log "platform: ${OS}/${ARCH}"

case "${OS}" in
  Linux | Darwin) ;;
  MINGW* | MSYS* | CYGWIN*)
    die "Windows native shell is not supported yet. Use WSL2, then re-run this script."
    ;;
  *)
    warn "untested OS '${OS}' — continuing; need a Unix-like environment with cargo"
    ;;
esac

# ─── rust toolchain ──────────────────────────────────────────────────────────

ensure_rust() {
  local rustc_line cargo_line rustc_ver cargo_ver
  if command -v cargo >/dev/null 2>&1 && command -v rustc >/dev/null 2>&1; then
    rustc_line="$(rustc --version 2>/dev/null || true)"
    cargo_line="$(cargo --version 2>/dev/null || true)"
    rustc_ver="$(tool_semver "${rustc_line}")"
    cargo_ver="$(tool_semver "${cargo_line}")"
    log "rustc ${rustc_ver}"
    log "cargo ${cargo_ver}"
    return 0
  fi

  if [[ -f "${HOME}/.cargo/env" ]]; then
    # shellcheck disable=SC1091
    . "${HOME}/.cargo/env"
  fi
  if command -v cargo >/dev/null 2>&1; then
    log "loaded cargo from ~/.cargo/env"
    return 0
  fi

  log "Rust toolchain not found; installing via rustup (https://rustup.rs)"
  need_cmd curl

  if [[ "${YES}" -ne 1 && -t 0 ]]; then
    printf 'Install rustup into %s? [Y/n] ' "${CARGO_HOME:-${HOME}/.cargo}"
    read -r ans || true
    case "${ans:-Y}" in
      n | N | no | NO) die "cargo is required to build DevCage" ;;
      *) ;;
    esac
  fi

  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  # shellcheck disable=SC1091
  . "${HOME}/.cargo/env"
  command -v cargo >/dev/null 2>&1 || die "rustup finished but cargo is still not on PATH"
  rustc_line="$(rustc --version 2>/dev/null || true)"
  rustc_ver="$(tool_semver "${rustc_line}")"
  log "rustc ${rustc_ver}"
}

check_linker_hint() {
  if command -v cc >/dev/null 2>&1 || command -v gcc >/dev/null 2>&1 || command -v clang >/dev/null 2>&1; then
    return 0
  fi
  case "${OS}" in
    Darwin)
      warn "no C compiler on PATH — install Xcode CLT: xcode-select --install"
      ;;
    Linux)
      warn "no C compiler on PATH — install build tools (e.g. sudo apt install build-essential)"
      ;;
    *) ;;
  esac
}

# ─── source tree ─────────────────────────────────────────────────────────────

resolve_source() {
  if [[ "${LOCAL_ONLY}" -eq 1 ]]; then
    local here
    here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    if [[ ! -f "${here}/Cargo.toml" ]]; then
      die "--local set but Cargo.toml not found next to scripts/ (${here})"
    fi
    SRC_DIR="${here}"
    log "building from local tree: ${SRC_DIR}"
    return 0
  fi

  need_cmd git

  mkdir -p "$(dirname "${SRC_DIR}")"
  if [[ -d "${SRC_DIR}/.git" ]]; then
    log "updating existing checkout: ${SRC_DIR}"
    git -C "${SRC_DIR}" remote set-url origin "${REPO}" 2>/dev/null || true
    git -C "${SRC_DIR}" fetch --tags --force origin
    if git -C "${SRC_DIR}" rev-parse --verify "refs/remotes/origin/${REF}" >/dev/null 2>&1; then
      git -C "${SRC_DIR}" checkout -B "${REF}" "origin/${REF}"
    elif git -C "${SRC_DIR}" rev-parse --verify "refs/tags/${REF}" >/dev/null 2>&1; then
      git -C "${SRC_DIR}" checkout --detach "refs/tags/${REF}"
    elif git -C "${SRC_DIR}" rev-parse --verify "${REF}" >/dev/null 2>&1; then
      git -C "${SRC_DIR}" checkout --detach "${REF}"
    else
      die "ref not found after fetch: ${REF}"
    fi
  else
    log "cloning ${REPO} (${REF}) → ${SRC_DIR}"
    rm -rf "${SRC_DIR}"
    if git ls-remote --exit-code --heads "${REPO}" "${REF}" >/dev/null 2>&1; then
      git clone --depth 1 --branch "${REF}" "${REPO}" "${SRC_DIR}"
    elif git ls-remote --exit-code --tags "${REPO}" "${REF}" >/dev/null 2>&1 \
      || git ls-remote --exit-code --tags "${REPO}" "${REF}^{}" >/dev/null 2>&1; then
      git clone --depth 1 --branch "${REF}" "${REPO}" "${SRC_DIR}"
    else
      git clone "${REPO}" "${SRC_DIR}"
      git -C "${SRC_DIR}" checkout "${REF}"
    fi
  fi

  [[ -f "${SRC_DIR}/Cargo.toml" ]] || die "clone missing Cargo.toml"
}

# ─── build & install ─────────────────────────────────────────────────────────

build_release() {
  log "cargo build --release (this may take a few minutes on first build)"
  (
    cd "${SRC_DIR}"
    cargo build --release --locked 2>/dev/null || cargo build --release
  )
  local built="${SRC_DIR}/target/release/devcage"
  if [[ ! -x "${built}" && -f "${built}.exe" ]]; then
    built="${built}.exe"
  fi
  [[ -x "${built}" ]] || die "build finished but binary missing: ${SRC_DIR}/target/release/devcage"
  BUILT_BIN="${built}"
}

install_binary() {
  log "installing → ${BIN}"
  mkdir -p "${BINDIR}"
  if command -v install >/dev/null 2>&1; then
    install -m 755 "${BUILT_BIN}" "${BIN}"
  else
    cp "${BUILT_BIN}" "${BIN}"
    chmod 755 "${BIN}"
  fi
  [[ -x "${BIN}" ]] || die "install failed: ${BIN} is not executable"
}

verify() {
  local ver
  ver="$("${BIN}" --version 2>/dev/null || true)"
  if [[ -z "${ver}" ]]; then
    warn "binary installed but --version failed"
  else
    log "ok: ${ver}"
  fi

  case ":${PATH}:" in
    *":${BINDIR}:"*) ;;
    *)
      warn "${BINDIR} is not on your PATH"
      printf '\n  Add this to your shell rc (bashrc/zshrc/profile):\n\n'
      printf "    export PATH=\"%s:\$PATH\"\n\n" "${BINDIR}"
      ;;
  esac
}

cleanup_src() {
  [[ "${KEEP_SRC}" -eq 1 ]] && return 0
  [[ "${LOCAL_ONLY}" -eq 1 ]] && return 0
  log "removing source checkout (${SRC_DIR})"
  rm -rf "${SRC_DIR}"
}

main() {
  log "DevCage install-from-source"
  check_linker_hint
  ensure_rust
  resolve_source
  build_release
  install_binary
  verify
  cleanup_src

  cat <<EOF

Installed: ${BIN}

Quick checks:
  devcage --version
  devcage doctor
  devcage verify

Daily:
  devcage grok
  devcage wrap -- <agent>
  devcage run -p offline -- echo hi

Docs: https://github.com/suryan/devcage  ·  docs/usage.md

EOF
}

main
