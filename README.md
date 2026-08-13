# DevCage

**Process wrap + disposable cages for AI agents** — Grok, Claude, Cursor, Codex, or any CLI. Not a replacement for your MCP/key stack.

DevCage is **not Grok-only**. Grok has the smoothest daily command (`devcage grok`) because it has its own `--sandbox`. Other agents use **`devcage wrap`**. Untrusted/offline/Linux **commands** use **`devcage run`**. Details: [docs/usage.md](docs/usage.md), [docs/recommended-stack.md](docs/recommended-stack.md).

> **Status:** **v0.6** — container + host backends; Grok mode + generic wrap. See the [PRD](docs/product_requirements.md).

| | |
|--|--|
| **Product** | DevCage |
| **CLI** | `devcage` |
| **Language** | Rust |
| **Agents** | **Any** CLI — Grok (first-class), others via `wrap` |
| **Deep cage** | `run -p offline\|untrusted\|agent` (container) |

## Pitch

```text
# Grok (uses Grok --sandbox + DevCage wiring)
devcage grok

# Any other agent (DevCage Seatbelt — no agent sandbox required)
devcage wrap -- claude
devcage wrap -- cursor-agent
devcage wrap --net none -- codex

# Deep cage for a command (any agent / any shell)
devcage run -p offline -- ./sketchy.sh
devcage run -p untrusted -- sh
devcage stop --wipe
```

| Piece | Job |
|------|-----|
| **`devcage grok`** | Daily Grok: agent `--sandbox` + recipes + safe env |
| **`devcage wrap`** | Daily *other* agents: DevCage process Seatbelt (macOS) |
| **`devcage run`** | Disposable Linux cage for a **command** |
| **mcp-secret-launcher** | Keys in Keychain → MCP env only |
| **mcp-guard** | MCP tool policy / HITL |
| **pkg-guard** | `uvx` / `npx` supply chain |

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

## Product shape (v0.6)

| In scope | Later |
|----------|--------|
| **`devcage grok`** + **`devcage wrap`** | Linux bwrap wrap |
| Profiles: `agent`, `offline`, `untrusted`, `host`, … | GUI / mobile special profiles |
| Container backend (Docker/Podman) + macOS Seatbelt wrap | VM |
| Project RW + ephemeral home + secret denylist | COW workspace, durable cages |
| Net: container `none`/`full`; wrap `--net none` | Kernel allowlist |
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
| [docs/recommended-stack.md](docs/recommended-stack.md) | Daily Grok + **wrap** for other agents; keys stack |
| [docs/development.md](docs/development.md) | Build, lint, coverage, pre-commit, releases |
| [docs/product_requirements.md](docs/product_requirements.md) | Goals, FR/NFR, agent wedge, MVP |
| [docs/architecture.md](docs/architecture.md) | Design + closed decisions |
| [docs/usage.md](docs/usage.md) | **CLI + Grok + wrap** — what each step means |
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

[MIT](LICENSE) — see `LICENSE` and `license = "MIT"` in `Cargo.toml`.
