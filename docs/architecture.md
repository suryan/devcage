# Architecture

**Status:** **MVP runtime implemented** — container + host backends; agent-first wedge.

See also:

- [product_requirements.md](./product_requirements.md) — goals, MVP, FR
- [usage.md](./usage.md) — CLI surface
- [workloads-gui-mobile.md](./workloads-gui-mobile.md) — **phase 3 appendix** (not core)
- [architecture.d2](./architecture.d2) / [flow.d2](./flow.d2) — diagrams

```bash
d2 docs/architecture.d2 docs/architecture.svg
d2 docs/flow.d2 docs/flow.svg
```

## North star

> Disposable **execution venues** for agent/dev work: project-scoped, no host secrets mounted, wipe when done.

**Preferred daily model for Grok** is **not** “put the whole agent in a container.” It is **process containment of Grok’s built-ins** plus DevCage only for deep/untrusted jobs. See **[recommended-stack.md](./recommended-stack.md)**.

```text
# Daily (Grok)
devcage grok  →  grok --sandbox keys-first + recipes + DevCage-ready env

# Other agents (no native sandbox)
devcage wrap [--net none] -- <agent>  →  Seatbelt (macOS) + env scrub

# Deep cage (any agent session)
devcage run  →  profile  →  container|host backend  →  command
                    ↑
         project RW + ephemeral home + secret denylist
```

## Overview

| Piece | Role |
|-------|------|
| **CLI** | **`grok`** (daily Grok), **`wrap`** (any agent), `run`, `shell`, `agent`, `limits`, `stop`, `doctor` |
| **Profile** | Fixed core set first: `agent`, `offline`, `host` |
| **Session** | id, workspace, ephemeral home, stop / wipe |
| **Backend** | MVP: **container** only |
| **Config** | env / `local.env`; optional minimal `.devcage.toml` later |

## Layering with the rest of the stack

```text
Grok (host, built-in tools)
  ├─ devcage grok → --sandbox / permission rules
  ├─ MCP → mcp-guard → secret-launcher → MCP servers
  ├─ PATH: pkg-guard shims (uvx/npx)
  └─ optional: devcage run            (deep container venue)

Other agents
  └─ devcage wrap [--net none] -- <cli>
       └─ optional: devcage run
```

None of these replace each other. Details: [recommended-stack.md](./recommended-stack.md).

## Module map (`src/`)

| Module | Responsibility |
|--------|----------------|
| `cli` | clap surface |
| `profiles` | built-in policies (`agent` / `offline` / `host`) |
| `plan` | mounts + `NetMode` |
| `session` | JSON session store + ephemeral home |
| `backend` | trait + **container** + **host** |
| `orchestrate` | run / agent / shell / stop / doctor / status |
| `config` | env + data dir |
| `banner` | start summary |

## Backends

| Backend | MVP | Notes |
|---------|-----|--------|
| `container` | **Yes** | Docker/Podman; order: `DEVCAGE_CONTAINER_RUNTIME`, podman, docker |
| `bwrap` | **Wrap only** | `devcage wrap` on Linux when `bwrap` is on PATH; argv plan unit-tested on macOS |
| `vm` / `native-mac` | No | Later / phase 3 |

**Fail closed** if container runtime missing for non-`host` profiles.

**Threat honesty:** containers share the host kernel (on macOS: Linux-in-VM + container). Not a micro-VM claim.

## Design decisions (closed)

### Product wedge (v0.2 product decision)

| # | Decision |
|---|----------|
| P1 | **Job:** easy + secure isolation for **AI agent development** |
| P2 | **Core profiles only:** `agent`, `offline`, `host` |
| P3 | **Happy path:** `devcage agent` — not a generic orchestrator UX |
| P4 | **GUI/mobile:** phase 3 appendix only; hybrid host tooling until then |
| P5 | **No custom profile DSL in v0.1** — fixed builtins first |
| P6 | **Honest network labels** — `agent` = full container net until allowlist is real |

### Technical (from earlier Section 12)

| # | Topic | Decision |
|---|--------|----------|
| 1 | MVP backend | **Container-first** |
| 2 | Network | Backend-native; enforce **`none`** + unrestricted container for `agent`; no MITM proxy; allowlist later |
| 3 | Default profile | **`agent`** (not a fake “allowlist” name) |
| 4 | Workspace | **RW project** default; COW later |
| 5 | Package caches | Optional **RO named** host caches; writes to session home |
| 6 | CLI name | **`devcage`** |

### UX security defaults

| Default | Value |
|---------|--------|
| Workspace | cwd (or git root later) |
| Home | ephemeral |
| Secrets | never mount `~/.ssh`, `~/.aws`, `~/.gnupg`, Keychain, browser profiles |
| Start banner | profile, workspace, home, network label, secrets: not mounted |
| Missing Docker | fail closed + `doctor` hint |

## Core profiles

```text
agent    → FS: project RW + ephemeral home + secret denylist
           net: full (container) — banner says so
offline  → same FS; net: none (enforced)
host     → break-glass; loud warnings
```

## Filesystem plan (MVP)

| Mount | Default |
|-------|---------|
| Project workspace | RW |
| Session home | Ephemeral RW |
| Host `$HOME` | Not mounted wholesale |
| Secret paths | Denied |
| Named package caches | Optional RO |
| COW overlay | Out of v0.1 |

## Network plan (MVP)

| Profile | Net | Claim |
|---------|-----|--------|
| `offline` | none | **Enforced** |
| `agent` | full container | **Labeled** — not “allowlist” |
| `host` | unrestricted | Explicit |

Phase 2: real allowlist/categories when testable.

## GUI and mobile

**Not core product.** See [workloads-gui-mobile.md](./workloads-gui-mobile.md) (phase 3).
Until then: run agents in `agent`/`offline`; keep Xcode / Studio / emulators on the host.

## Configuration

Paths and third-party URLs via env / `local.env.example`. Never commit secrets or machine-specific absolute paths.

## Security boundary (summary)

**In cage:** agent process tree, ephemeral home, project mount, network per profile.
**Trusted host surface:** explicit mounts, container runtime, kernel.
**Not a silver bullet:** prompt injection can still edit the **mounted project**; `host` and `docker.sock` are break-glass; container ≠ VM.
