#!/usr/bin/env bash
# Bump Cargo package version, refresh CHANGELOG, write releases/vX.Y.Z.md
#
# Usage:
#   ./scripts/bump-version.sh patch|minor|major [notes...]
#   ./scripts/bump-version.sh 0.2.0 "Optional summary line"
#
# Does not create a git tag or commit (caller / agent commits after change).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 patch|minor|major|X.Y.Z [release notes...]" >&2
  exit 2
fi

KIND="$1"
shift
NOTES="${*:-}"

CARGO_TOML="${ROOT}/Cargo.toml"
CHANGELOG="${ROOT}/CHANGELOG.md"
RELEASES_DIR="${ROOT}/releases"

current="$(sed -n 's/^version = "\([0-9][^"]*\)"/\1/p' "${CARGO_TOML}" | head -n1)"
if [[ -z "${current}" ]]; then
  echo "error: could not read version from Cargo.toml" >&2
  exit 1
fi

IFS=. read -r MAJOR MINOR PATCH <<<"${current}"
MAJOR="${MAJOR//[^0-9]/}"
MINOR="${MINOR//[^0-9]/}"
PATCH="${PATCH//[^0-9]/}"

case "${KIND}" in
  patch)
    PATCH=$((PATCH + 1))
    new="${MAJOR}.${MINOR}.${PATCH}"
    ;;
  minor)
    MINOR=$((MINOR + 1))
    PATCH=0
    new="${MAJOR}.${MINOR}.${PATCH}"
    ;;
  major)
    MAJOR=$((MAJOR + 1))
    MINOR=0
    PATCH=0
    new="${MAJOR}.${MINOR}.${PATCH}"
    ;;
  *)
    if [[ "${KIND}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
      new="${KIND}"
    else
      echo "error: expected patch|minor|major|X.Y.Z, got: ${KIND}" >&2
      exit 2
    fi
    ;;
esac

if [[ "${new}" == "${current}" ]]; then
  echo "error: new version equals current (${current})" >&2
  exit 1
fi

echo "==> bump ${current} -> ${new}"

tmp="$(mktemp)"
awk -v ver="${new}" '
  BEGIN { done=0 }
  /^version = "/ && done==0 {
    print "version = \"" ver "\""
    done=1
    next
  }
  { print }
' "${CARGO_TOML}" >"${tmp}"
mv "${tmp}" "${CARGO_TOML}"

today="$(date -u +%Y-%m-%d)"
mkdir -p "${RELEASES_DIR}"
release_file="${RELEASES_DIR}/v${new}.md"

{
  echo "# DevCage v${new}"
  echo
  echo "Released: ${today}"
  echo
  if [[ -n "${NOTES}" ]]; then
    echo "## Summary"
    echo
    echo "${NOTES}"
    echo
  fi
  echo "## Changes"
  echo
  echo "- See [CHANGELOG.md](../CHANGELOG.md) entry for ${new}."
  echo
  echo "## Install"
  echo
  echo '```bash'
  echo "./scripts/build.sh --release"
  echo "./scripts/install.sh"
  echo '```'
  echo
  echo "## Verification"
  echo
  echo '```bash'
  echo "./scripts/check.sh"
  echo "devcage doctor"
  echo '```'
} >"${release_file}"

if [[ ! -f "${CHANGELOG}" ]]; then
  cat >"${CHANGELOG}" <<EOF
# Changelog

All notable changes to DevCage are documented here.

Format inspired by [Keep a Changelog](https://keepachangelog.com/).
Versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [${new}] - ${today}

### Added

- Release ${new}.

EOF
else
  tmp="$(mktemp)"
  awk -v ver="${new}" -v day="${today}" -v notes="${NOTES}" '
    BEGIN { inserted=0 }
    /^## \[Unreleased\]/ && inserted==0 {
      print
      print ""
      print "## [" ver "] - " day
      print ""
      if (notes != "") {
        print "### Notes"
        print ""
        print notes
        print ""
      }
      print "### Changed"
      print ""
      print "- Version bump to " ver " (see releases/v" ver ".md)."
      print ""
      inserted=1
      next
    }
    { print }
  ' "${CHANGELOG}" >"${tmp}"
  mv "${tmp}" "${CHANGELOG}"
fi

echo "==> wrote ${release_file}"
echo "==> updated CHANGELOG.md and Cargo.toml"
echo
echo "Next:"
echo "  1. Edit CHANGELOG.md / releases/v${new}.md with real notes"
echo "  2. ./scripts/check.sh"
echo "  3. git add -A && git commit -m \"release: v${new}\""
echo "  4. git tag v${new}   # when you are ready to tag"
