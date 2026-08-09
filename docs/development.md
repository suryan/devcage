# Development

DevCage is a **Rust** CLI. This document covers building, **strict quality gates**, releases, and contribution habits.

## Prerequisites

| Tool | Purpose |
|------|---------|
| Rust stable (1.74+) via [rustup](https://rustup.rs) | Build |
| `rustfmt`, `clippy`, `llvm-tools-preview` | Format, lint, coverage |
| [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov) | Coverage gate |
| [ShellCheck](https://www.shellcheck.net) | Script lint |
| [pre-commit](https://pre-commit.com) (recommended) | Git hooks |
| Docker **or** Podman (for isolated profiles) | Container backend |
| Colima (macOS helper) | `./scripts/setup-runtime.sh` |

```bash
rustup component add rustfmt clippy llvm-tools-preview
cargo install cargo-llvm-cov --locked
brew install shellcheck pre-commit   # or equivalent
./scripts/setup-hooks.sh
cp local.env.example local.env       # optional

# macOS container runtime (recommended for agent/offline/untrusted):
./scripts/setup-runtime.sh           # Colima + Docker CLI
# export DOCKER_HOST=unix://${HOME}/.colima/docker.sock
```

## Layout

```text
.
├── AGENTS.md           # project rules (agents + humans)
├── CHANGELOG.md
├── releases/           # per-version release notes
├── Cargo.toml          # includes strict [lints.*]
├── rustfmt.toml
├── clippy.toml
├── .pre-commit-config.yaml
├── docs/
├── scripts/
│   ├── check.sh        # full gate
│   ├── coverage.sh
│   ├── lint-scripts.sh
│   ├── pre-commit.sh
│   ├── setup-hooks.sh
│   ├── bump-version.sh
│   ├── build.sh
│   └── install.sh
└── src/
```

## Quality gate (required)

```bash
./scripts/check.sh           # fmt + clippy + shellcheck + test + coverage
./scripts/check.sh --quick   # skip coverage (local only; not for commit)
```

| Check | Tool | Policy |
|-------|------|--------|
| Format | `cargo fmt --check` | Required |
| Lint | Clippy **`all` + `pedantic` (deny)** + nursery/cargo + selected restriction | `-D warnings` |
| Scripts | ShellCheck `--severity=style --enable=all` | Required |
| Tests | `cargo test` | Required |
| Coverage | `cargo llvm-cov --fail-under-lines` | Default **80%** (`DEVCAGE_COVERAGE_MIN`) |

Rust lint details live in `Cargo.toml` `[lints.rust]` / `[lints.clippy]`.

**Quality rules:**

- Prefer **fixing** over `#[allow(clippy::…)]`. Pedantic is fully denied — do not “skip” it with blanket allows.
- Layout is **`lib` + thin `main`** so crate-private modules use correct visibility under strict Clippy.
- Policy knobs use **enums** (not bool piles) to stay pedantic-clean.
- The only intentional Clippy allows are CLI `print_stdout` / `print_stderr` (product surface) and transitive `multiple_crate_versions`.

### Coverage

```bash
./scripts/coverage.sh
# DEVCAGE_COVERAGE_MIN=85 ./scripts/coverage.sh
```

Report: `target/coverage/lcov.info` (gitignored).

## Pre-commit

```bash
./scripts/setup-hooks.sh
pre-commit run --all-files   # optional manual run
```

Hooks run the **full** `scripts/check.sh` (including coverage). Do not `--no-verify` unless the user asks.

## Build & run

```bash
./scripts/build.sh
./scripts/build.sh --release
cargo run -- doctor
cargo run -- profile list
cargo run -- agent           # not implemented yet (scaffold)
```

## Changelog and releases

1. While developing, append bullets under **`## [Unreleased]`** in `CHANGELOG.md`.
2. Ship a version:
   ```bash
   ./scripts/bump-version.sh patch "Short summary of the release"
   # or: minor | major | 0.2.0
   ```
3. Edit `CHANGELOG.md` and `releases/vX.Y.Z.md` for accuracy.
4. `./scripts/check.sh` then commit (e.g. `release: vX.Y.Z`).
5. Tag when ready: `git tag vX.Y.Z`.

## Project rules

See **[AGENTS.md](../AGENTS.md)** for mandatory agent/human habits, including:

- Always run the quality gate before commit
- **Always commit after a coherent change**
- Keep the product wedge (agent isolation) small

## Logging

```bash
RUST_LOG=debug cargo run -- doctor
```

## Implementing features

1. Read [product_requirements.md](./product_requirements.md) and [architecture.md](./architecture.md).
2. Modules under `src/`; automation under `scripts/`; docs under `docs/`.
3. Paths from env / project root — never personal absolute paths.
4. Update `CHANGELOG.md` `[Unreleased]`; commit when done.
