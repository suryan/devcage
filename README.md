# DevCage

**Optional deep cage for untrusted / offline / Linux commands** — not a replacement for Grok’s built-in tools or your MCP/key stack.

> **Recommended daily model (Grok):** keep built-ins; contain the **process** with Grok `--sandbox` + permissions; keep keys via **mcp-secret-launcher** + **mcp-guard**; packages via **pkg-guard**. Use DevCage when you need a disposable container venue. See [docs/recommended-stack.md](docs/recommended-stack.md).

> **Status:** **MVP** — container + host backends. See the [PRD](docs/product_requirements.md).

| | |
|--|--|
| **Product** | DevCage |
| **CLI** | `devcage` |
| **Language** | Rust |
| **Daily path** | Grok sandbox + your secret/MCP/package tools |
| **DevCage role** | Explicit caged `run` / `agent` when host sandbox is not enough |

## Pitch

```text
# Daily Grok (uses Grok's own --sandbox + DevCage wiring):
devcage grok

# Any other agent (DevCage applies Seatbelt — agent needs no sandbox mode):
devcage wrap -- claude
devcage wrap -- cursor-agent

# Deep cage when needed:
devcage run -p offline -- ./sketchy.sh
devcage run -p untrusted -- sh
devcage stop --wipe
```

| Tool | Job |
|------|-----|
| **Grok sandbox + permissions** | Limit what built-in tools can touch |
| **mcp-secret-launcher** | Keys in Keychain → MCP env only |
| **mcp-guard** | MCP tool policy / HITL |
| **pkg-guard** | `uvx` / `npx` supply chain |
| **DevCage** | Where a **command** runs (container/offline/untrusted) |

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
| [docs/recommended-stack.md](docs/recommended-stack.md) | **Daily Grok model** — sandbox + keys stack; DevCage optional |
| [docs/development.md](docs/development.md) | Build, lint, coverage, pre-commit, releases |
| [docs/product_requirements.md](docs/product_requirements.md) | Goals, FR/NFR, agent wedge, MVP |
| [docs/architecture.md](docs/architecture.md) | Design + closed decisions |
| [docs/usage.md](docs/usage.md) | **CLI + Grok guide** — step-by-step with *what each step means* |
| [CHANGELOG.md](CHANGELOG.md) | Keep-a-Changelog history |
| [docs/agent-images.md](docs/agent-images.md) | Container images + host-restricted agents |
| [examples/grok/](examples/grok/) | Copy-paste Grok sandbox + permission recipes |
| [docs/workloads-gui-mobile.md](docs/workloads-gui-mobile.md) | Phase 3 appendix (not core) |

## Complements

- [pkg-guard](https://github.com/suryan/pkg-guard) — package **policy**
- [mcp-secret-launcher](https://github.com/suryan/mcp-secret-launcher) — Keychain → MCP env
- [mcp-guard](https://github.com/suryan/mcp-guard) — MCP L7 policy / HITL
- DevCage — optional **command venue** (container / offline / untrusted)

## License

MIT (see `Cargo.toml`).
