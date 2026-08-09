# DevCage

**Easy isolation for AI agent development** — run Grok, Claude Code, Gemini CLI (and friends) so they can edit **this project**, not your home directory or secrets. When you are done, wipe the session.

> **Status:** **MVP** — container isolation for `agent`/`offline` (Docker/Podman); break-glass `host`. See the [PRD](docs/product_requirements.md).

| | |
|--|--|
| **Product** | DevCage |
| **CLI** | `devcage` |
| **Language** | Rust |
| **North star** | One command → agent in a disposable cage |

## Pitch

```text
devcage agent          # project RW, no host secrets, ephemeral home
devcage agent --offline
devcage stop --wipe
```

**pkg-guard** (optional) decides whether a package is allowed.
**DevCage** decides *where* the agent runs and makes that space easy to discard.

## Quick start

```bash
cp local.env.example local.env   # optional
./scripts/setup-hooks.sh         # pre-commit quality gate
./scripts/build.sh
cargo run -- doctor
cargo run -- verify
cargo run -- run -p host -- /usr/bin/true   # break-glass (no Docker)
# requires Docker or Podman:
cargo run -- pull
cargo run -- run -p offline -- echo hi
cargo run -- run -p untrusted -- sh -c 'echo safe'
./scripts/e2e-container.sh                  # skips if no runtime
```

Quality gate (required before commit):

```bash
./scripts/check.sh
```

Release install (prefix overridable; default `$HOME/.local`):

```bash
./scripts/install.sh
```

## Product shape (v0.1 focus)

| In scope | Out of v0.1 (roadmap only) |
|----------|----------------------------|
| Profiles: **`agent`**, **`offline`**, **`host`** | GUI / mobile special profiles |
| Happy path: **`devcage agent`** | Custom profile DSL, multi-backend |
| Container backend (Docker/Podman) | bwrap, VM, native-mac |
| Project RW + ephemeral home + secret denylist | COW workspace, durable cages |
| Net: **none** (`offline`) or **full container** (`agent`) — labeled honestly | Domain allowlist / categories |
| Optional RO host package caches | pkg-guard required integration |

Closed decisions: [docs/architecture.md](docs/architecture.md#design-decisions-closed).

## Layout

```text
.
├── README.md
├── AGENTS.md              # project rules (agents + humans)
├── CHANGELOG.md
├── releases/              # vX.Y.Z release notes
├── Cargo.toml             # strict Clippy lints
├── .pre-commit-config.yaml
├── local.env.example
├── docs/
├── scripts/               # check, coverage, bump-version, hooks, …
└── src/
```

## Docs

| Doc | Contents |
|-----|----------|
| [AGENTS.md](AGENTS.md) | **Project rules** — quality gate, commit habit, changelog |
| [docs/development.md](docs/development.md) | Build, lint, coverage, pre-commit, releases |
| [docs/product_requirements.md](docs/product_requirements.md) | Goals, FR/NFR, agent wedge, MVP |
| [docs/architecture.md](docs/architecture.md) | Design + closed decisions |
| [docs/usage.md](docs/usage.md) | CLI usage (scaffold + intended) |
| [CHANGELOG.md](CHANGELOG.md) | Keep-a-Changelog history |
| [docs/workloads-gui-mobile.md](docs/workloads-gui-mobile.md) | Phase 3 appendix (not core) |

## Complements

- [pkg-guard](https://github.com/suryan/pkg-guard) — package **policy** (optional)
- DevCage — execution **venue** (containment)

## License

MIT (see `Cargo.toml`).
