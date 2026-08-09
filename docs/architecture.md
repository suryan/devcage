# Architecture

**Status:** Scaffold — **agent-first wedge** + closed MVP decisions. Runtime backends not implemented yet.

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

> One command runs an AI agent in a disposable cage: it can edit this project, not the host home or secrets; wipe when done.

```text
devcage agent  →  profile=agent  →  container backend  →  agent process
                      ↑
              project RW + ephemeral home + secret denylist
```

## Overview

| Piece | Role |
|-------|------|
| **CLI** | **`agent`** (happy path), `run`, `shell`, `stop`, `status`, `doctor`, `profile` |
| **Profile** | Fixed core set first: `agent`, `offline`, `host` |
| **Session** | id, workspace, ephemeral home, stop / wipe |
| **Backend** | MVP: **container** only |
| **Config** | env / `local.env`; optional minimal `.devcage.toml` later |

## Layering with pkg-guard (optional)

```text
Host PATH
  optional: pkg-guard shims (uvx/npx policy)
       → devcage agent|run (FS/net cage)
            → agent / shell / package manager
```

Neither requires the other.

## Module map (planned under `src/`)

| Module | Responsibility |
|--------|----------------|
| `cli` | clap surface (present) |
| `profile` | built-in core profiles (minimal schema in v0.1) |
| `session` | lifecycle, metadata, wipe |
| `backend` | trait + **container** impl first |
| `fs` | mount plan, ephemeral home, secret denylist |
| `net` | `none` vs full container net (allowlist later) |
| `config` | env + config dirs (no hardcoded machine paths) |

## Backends

| Backend | MVP | Notes |
|---------|-----|--------|
| `container` | **Yes** | Docker/Podman; order: `DEVCAGE_CONTAINER_RUNTIME`, podman, docker |
| `bwrap` | No | Linux fast-follow after agent UX is solid |
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
