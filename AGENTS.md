# DevCage — project rules for humans and agents

These rules apply to **this repository**. Follow them on every change.

## Product focus

- **Job:** easy, secure isolation for **AI agent development**.
- **Core profiles:** `agent`, `offline`, `host` only for v0.1.
- **Happy path:** `devcage agent`.
- GUI/mobile designs live in `docs/workloads-gui-mobile.md` as **phase 3**, not core work.

## Code quality (strict)

1. **Rust**
   - `unsafe` is **denied**.
   - Clippy: **all + pedantic + nursery + cargo + selected restriction** (see `Cargo.toml`).
   - `cargo fmt` must be clean.
   - Prefer small testable modules under `src/`; keep `main.rs` thin.
2. **Coverage**
   - Line coverage gate: **`DEVCAGE_COVERAGE_MIN` default 80%** (`scripts/coverage.sh`).
   - New logic needs tests (unit and/or `tests/` integration).
3. **Shell scripts**
   - All automation under `scripts/`.
   - Must pass **ShellCheck** (`scripts/lint-scripts.sh`).
   - `set -euo pipefail`; no hardcoded machine home paths in committed files.
4. **Full gate**
   - Before finish: `./scripts/check.sh` (fmt, clippy, shellcheck, test, coverage).
   - Iteration only: `./scripts/check.sh --quick` then full check before commit.

## Pre-commit

- Install hooks once: `./scripts/setup-hooks.sh`
- Pre-commit runs the **full quality gate** (including coverage).
- Do not use `--no-verify` unless the user explicitly asks.

## Changelog, releases, versions

1. **Every user-visible or behavior change** updates `[Unreleased]` in `CHANGELOG.md`.
2. **Release notes** for shipped versions live under `releases/vX.Y.Z.md`.
3. **Bump versions** only via:
   ```bash
   ./scripts/bump-version.sh patch|minor|major "optional summary"
   ```
   Then edit the generated CHANGELOG/release notes, run `./scripts/check.sh`, commit.
4. Semantic versioning: fix → patch, feature → minor, break → major.

## Git habit (mandatory in this project)

1. **After a coherent change set, always commit** (do not leave large finished work uncommitted).
2. Run `./scripts/check.sh` (or pre-commit) **before** the commit.
3. Commit messages: complete sentences; explain *why* when non-obvious.
4. Do **not** force-push, amend published history, or skip hooks without explicit user request.
5. Do **not** commit secrets or `local.env`.

## Config and layout

- Follow global layout: `README.md`, `docs/`, `scripts/`, `src/`, `local.env.example`.
- No hardcoded `/Users/...` paths in source, scripts, or docs.
- Third-party URLs and local paths: `local.env.example` / env vars.

## When implementing features

1. Read `docs/product_requirements.md` and `docs/architecture.md`.
2. Keep the agent wedge small; reject scope creep into mobile/GUI for v0.1.
3. Fail closed for isolation claims.
4. Update CHANGELOG `[Unreleased]`; commit when the change is complete.
