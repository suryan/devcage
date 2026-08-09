# Changelog

All notable changes to **DevCage** are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **MVP runtime:** container backend (Docker/Podman) for `agent` / `offline`; break-glass `host` backend.
- Working CLI: `doctor`, `status`, `profile`, `run`, `shell`, `agent`, `stop [--wipe]`.
- Session store under data dir; start banner with honest network labels; fail closed without runtime.
- Optional RO host package cache mounts (`DEVCAGE_SHARE_PACKAGE_CACHES`).
- Project quality gate: strict Clippy (pedantic/nursery + selected restriction), `rustfmt`, ShellCheck.
- Coverage gate via `cargo llvm-cov` (`scripts/coverage.sh`, default ≥ 80% lines).
- Pre-commit hooks (`./scripts/setup-hooks.sh`, `.pre-commit-config.yaml`).
- `CHANGELOG.md`, `releases/` notes, and `scripts/bump-version.sh`.
- Project rules in `AGENTS.md` (always run checks; always commit after changes).

## [0.1.0] - 2026-08-09

### Added

- Scaffold Rust CLI (`doctor`, `status`, `profile`, `agent`, `run`, `shell`, `stop`).
- Product docs: PRD, architecture, usage; agent-first wedge (`agent` / `offline` / `host`).
- Scripts: `build.sh`, `check.sh`, `install.sh`.
