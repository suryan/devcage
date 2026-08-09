#!/usr/bin/env bash
# Lint all project shell scripts with ShellCheck (strict).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

if ! command -v shellcheck >/dev/null 2>&1; then
  echo "error: shellcheck not found on PATH" >&2
  echo "install: brew install shellcheck   # or see https://www.shellcheck.net" >&2
  exit 1
fi

# Collect paths without process-substitution pipelines (ShellCheck SC2312).
list_file="$(mktemp)"
find scripts -type f -name '*.sh' | LC_ALL=C sort >"${list_file}"
scripts=()
while IFS= read -r f; do
  [[ -n "${f}" ]] && scripts+=("${f}")
done <"${list_file}"
rm -f "${list_file}"

if [[ ${#scripts[@]} -eq 0 ]]; then
  echo "error: no scripts/*.sh found" >&2
  exit 1
fi

echo "==> shellcheck (${#scripts[@]} scripts)"
# Severity: style; enable optional checks.
# SC1091: external sources (local.env) are intentional.
shellcheck \
  --shell=bash \
  --severity=style \
  --enable=all \
  --exclude=SC1091 \
  "${scripts[@]}"

echo "==> shell scripts ok"
