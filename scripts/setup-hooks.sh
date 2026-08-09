#!/usr/bin/env bash
# Install git pre-commit hooks for DevCage (pre-commit framework preferred).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

if command -v pre-commit >/dev/null 2>&1; then
  echo "==> pre-commit install"
  pre-commit install --install-hooks
  echo "==> hooks installed via pre-commit"
  echo "    run once: pre-commit run --all-files"
  exit 0
fi

# Fallback: install a local git hook
HOOK_DIR="${ROOT}/.git/hooks"
if [[ ! -d "${HOOK_DIR}" ]]; then
  echo "error: .git/hooks not found (not a git checkout?)" >&2
  exit 1
fi

HOOK="${HOOK_DIR}/pre-commit"
cat >"${HOOK}" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
exec "${ROOT}/scripts/pre-commit.sh"
EOF
chmod +x "${HOOK}"
echo "==> installed fallback git hook at .git/hooks/pre-commit"
echo "    prefer: brew install pre-commit && ./scripts/setup-hooks.sh"
