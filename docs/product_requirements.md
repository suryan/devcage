# DevCage — Product Requirements

**Product name:** DevCage
**Codename / repo:** `devcage`
**Status:** Draft PRD (v0.2 product wedge) — scaffold CLI; isolation not implemented
**Language:** Rust (`src/`, binary `devcage`)
**North star:** One command runs an AI agent in a disposable cage (edit project, not host secrets; wipe when done).
**Related:** Optional complement to [pkg-guard](https://github.com/suryan/pkg-guard) (package *policy*); DevCage is execution *venue*.
**Design decisions:** [architecture.md](./architecture.md#design-decisions-closed).

---

## 1. Problem statement

AI coding agents and modern package workflows routinely execute untrusted code on the developer’s machine:

- `uvx` / `npx` / `pip install` pull packages and **transitive** deps, often with install scripts
- Agents (Grok Build, Claude Code, Gemini CLI, etc.) run shell tools with broad host access
- A single malicious or compromised package can read `~/.ssh`, scrape tokens, or persist outside the project
- Stopping the agent does not reliably **undo** filesystem or credential damage on the host

Today’s mitigations are incomplete when used alone:

| Approach | Gap |
|----------|-----|
| Package policy (pkg-guard, Socket, audits) | Decides *whether* to install; does not contain *blast radius* if something slips through |
| Full VMs / “use Docker for everything” | Heavy, poorly profiled for AI CLIs + iOS Simulator edge cases |
| Hope + `.gitignore` | Not a security boundary |

**Core gap:** There is no first-class, **profiled, disposable developer / agent runtime** that:

1. Isolates work from the host OS by default
2. Applies **network and filesystem policy by profile** (offline, allowlisted online, special device access)
3. Survives package installs without permanently polluting the host
4. Can be **terminated and discarded** cleanly

---

## 2. Product vision

**DevCage** makes AI agent development **easy to isolate**: one command, a small set of profiles, disposable sessions.

> **North star:** One command runs my AI agent in a disposable cage — it can edit this project, not my home or secrets; wipe when done.
> **pkg-guard answers:** “Is this package allowed?”
> **DevCage answers:** “Where is it allowed to run, and what can it still touch?”

Users should be able to:

- Run `devcage agent` (or `agent --offline`) in a project
- Edit and build within the project envelope without host secrets
- `stop --wipe` and leave the **host home directory largely untouched**

---

## 3. Goals

1. **Easy happy path** — `devcage agent` for daily AI coding
2. **Containment by default** — project RW, no host secrets, ephemeral home
3. **Small profile set** — `agent`, `offline`, `host` (not a profile zoo)
4. **Disposable sessions** — `stop --wipe`
5. **Honest labels** — never claim allowlist when network is full container
6. **Optional pkg-guard** — compose when present; never require
7. **Honest threat model** — container residual risk documented

### Non-goals (v0.1 / core product)

- GUI / mobile / iOS Simulator as core features (phase 3 appendix only)
- Perfect domain allowlists on day one
- Replacing corporate EDR / MDM or Qubes-style multi-VM desktop
- Shipping a curated global malware feed
- Guaranteeing safety against kernel or hypervisor escapes
- Replacing Dev Containers as a team reproducibility product

---

## 4. Target users

| User | Need |
|------|------|
| Individual developers using AI CLIs | Run agents without trusting every `uvx`/`npx` with full home access |
| Security-conscious teams | Standard profiles for “agent work” vs “mobile sim” vs “offline review” |
| Platform / DX engineers | Wrap internal agent toolchains in a consistent isolation layer |
| pkg-guard users | Stack policy (names/OSV) with containment (FS/net/lifetime) |

---

## 5. Core concepts

### 5.1 Profile

A **profile** is a named policy bundle:

- Filesystem mounts (what is visible / writable)
- Network policy (none / allowlist / categories / host)
- Device / special access (none, iOS Simulator paths, Docker socket — explicit only)
- Process limits (optional: memory, PIDs, seccomp-class)
- Lifetime (ephemeral volume vs durable project cage)
- Entry commands allowed or recommended (shell, `grok`, `claude`, package managers)

### 5.2 Session

A **session** is a running instance of a profile:

- Started with an entrypoint (`shell`, agent CLI, or single command)
- Bound to a workspace (host project path projected in)
- Has an ID, status, and teardown action (`stop`, `stop --wipe`)

### 5.3 Backend

A **backend** implements profiles on a platform:

| Backend | Platform | Notes |
|---------|----------|--------|
| `bwrap` | Linux | bubblewrap namespaces; first-class on Linux |
| `container` | Linux + macOS | Docker / Podman / compatible runtime |
| `vm` (later) | macOS / Linux | Stronger isolation when containers insufficient |
| `native-mac` (later / experimental) | macOS | sandbox-exec / Seatbelt-inspired; best-effort |

**Portable product = profile schema + backends.** Not “everyone runs bwrap.”

### 5.4 Relationship to package managers and agents

```
Host PATH (optional)
  └─ pkg-guard shims (uvx/npx policy)
        └─ DevCage session (FS/net cage)
              └─ real uvx / npx / agent / shell
```

Installs inside the cage write to **cage-local** home/cache unless the profile explicitly shares host caches.

---

## 6. Functional requirements

### FR-1: Profile management

| ID | Requirement |
|----|-------------|
| FR-1.1 | Ship **core** built-ins: `agent`, `offline`, `host` |
| FR-1.2 | v0.1: fixed builtins only (no full custom profile DSL required) |
| FR-1.3 | Optional later: user/project overrides; validated declarative profiles |
| FR-1.4 | `devcage profile list` / `show` |
| FR-1.5 | GUI/mobile profiles are **phase 3 only** (see [workloads-gui-mobile.md](./workloads-gui-mobile.md)) |

### FR-2: Session lifecycle

| ID | Requirement |
|----|-------------|
| FR-2.0 | **`devcage agent [name]`** is the primary happy path (default profile `agent`) |
| FR-2.1 | `devcage run --profile <name> [--] <command…>` runs command inside profile |
| FR-2.2 | `devcage shell --profile <name>` interactive shell in profile |
| FR-2.3 | `devcage agent --offline` maps to offline profile |
| FR-2.4 | `devcage stop <session>` terminates processes in the session |
| FR-2.5 | `devcage stop --wipe` discards ephemeral volumes/homes for that session |
| FR-2.6 | Session metadata: id, profile, backend, workspace, started_at, status |
| FR-2.7 | Exit non-zero if profile cannot be applied (fail closed, not open) |

### FR-3: Filesystem policy

| ID | Requirement |
|----|-------------|
| FR-3.1 | Default: project workspace mounted read-write; host `$HOME` **not** fully mounted |
| FR-3.2 | Ephemeral per-session home for tool configs/caches unless profile opts into shared cache |
| FR-3.3 | Explicit allowlist of additional bind mounts (e.g. read-only SDK paths) |
| FR-3.4 | Deny-by-default for sensitive paths: `~/.ssh`, `~/.gnupg`, browser profiles, cloud creds dirs — only if profile explicitly allows |
| FR-3.5 | Optional durable “project cage” volume that persists across sessions but stays outside host home clutter |

### FR-4: Network policy

| ID | Requirement |
|----|-------------|
| FR-4.1 | Profile mode `none` — no network (`offline`) — **enforced in v0.1** |
| FR-4.2 | Profile `agent` uses full container network in v0.1, **labeled honestly** (not marketed as allowlist) |
| FR-4.3 | Profile mode `host` — unrestricted (escape hatch; must be explicit) |
| FR-4.4 | Phase 2: `allowlist` / `categories` only when testable — no MITM HTTPS proxy required for v1 |
| FR-4.5 | Residual gaps for future allowlist (raw IPs, DoH) documented when that mode ships |

### FR-5: Agent and tool entrypoints

| ID | Requirement |
|----|-------------|
| FR-5.1 | **`devcage agent`** launches default or named agent CLI (grok / claude / gemini via PATH or env) |
| FR-5.2 | Generic `devcage run --profile agent\|offline\|host -- <any-command>` |
| FR-5.3 | Docs for agent recipes; optional MCP notes (do not block MVP) |
| FR-5.4 | Optional pkg-guard composition when present — **not required** |

### FR-6: Package install isolation

| ID | Requirement |
|----|-------------|
| FR-6.1 | Installs inside a session do not write to host global site-packages / host `~/.local` unless profile shares those paths |
| FR-6.2 | Optional later `untrusted` profile: wipe-oriented, minimal mounts |
| FR-6.3 | Optional RO named host package caches for speed; writes stay in session home |

### FR-7: Special profiles (devices, GUI, mobile) — phase 3

Not v0.1 scope. Design appendix: [workloads-gui-mobile.md](./workloads-gui-mobile.md).

| ID | Requirement (when phase 3 is scheduled) |
|----|-------------|
| FR-7.1 | Never default; explicit profile only |
| FR-7.2 | Docker socket / ADB / live display / CoreSimulator = elevated trust, fail closed |
| FR-7.3 | Hybrid guidance until then: agent in cage, native IDEs/sims on host |

### FR-8: Observability and UX

| ID | Requirement |
|----|-------------|
| FR-8.1 | Human-readable **start banner**: profile, workspace, home, network label, secrets status |
| FR-8.2 | `devcage status` lists active sessions |
| FR-8.3 | Structured logs to stderr; optional JSON event log |
| FR-8.4 | `doctor`: container runtime, agent commands on PATH, known misconfigurations |
| FR-8.5 | Isolation smoke checks documented (e.g. secrets paths not readable in cage) |

---

## 7. Built-in profiles

### Core (v0.1)

| Profile | Network | FS summary | Intent |
|---------|---------|------------|--------|
| **`agent`** | full container (**labeled**) | project RW; ephemeral home; no host secrets | **Default** daily AI coding |
| **`offline`** | **none** (enforced) | same FS isolation | Review / local-only; no registry/API exfil |
| **`host`** | host | near-host | Explicit break-glass; loud warnings |

### Phase 2+

| Profile | Intent |
|---------|--------|
| `untrusted` | One-off package experiments; stricter wipe |
| future allowlist mode | Real egress policy when enforceable |

### Phase 3 (appendix only)

GUI/mobile profiles — see [workloads-gui-mobile.md](./workloads-gui-mobile.md). **Not** part of the core pitch.

---

## 8. Non-functional requirements

| ID | Requirement | Target |
|----|-------------|--------|
| NFR-1 | Session start overhead (warm backend) | &lt; 2s median on reference laptop for `bwrap`/`container` |
| NFR-2 | Fail closed | If backend cannot enforce net/FS policy, refuse start |
| NFR-3 | Single CLI UX across backends | Same profile names and flags |
| NFR-4 | No mandatory cloud account | Fully local operation |
| NFR-5 | Documentation of residual risks | Per backend, in-tree threat model doc |
| NFR-6 | Cross-platform roadmap | Linux MVP; macOS via container (or VM) in v1 or v1.1 |

---

## 9. Threat model (summary)

### In scope (mitigate)

- Malicious package install scripts reading host secrets via default home mounts
- Agent running `curl | bash` or broad FS reads outside the project
- Accidental persistence of untrusted toolchains in host user environment
- Data exfil to non-allowlisted hosts **when net policy is enforced**

### Out of scope / residual

- Kernel exploits, container breakouts, malicious privileged backends
- User-selected `host` profile or mounting `$HOME` / `docker.sock`
- Supply-chain **identity** fraud (typosquats) without package policy tools
- Compromised LLM provider accounts or prompt injection that stays inside the cage (cage limits damage to cage + mounted workspace)

**Prompt injection still matters:** an agent inside `net-allowlist` can still modify the **mounted project**. DevCage reduces host blast radius; it does not make the repo immune to malicious edits.

---

## 10. Success metrics

| Metric | Signal |
|--------|--------|
| Containment | Process in `agent`/`offline` cannot read host `~/.ssh` (tested) |
| Network policy | Process in `offline` cannot complete TCP to public internet (tested) |
| Disposable | `stop --wipe` removes session home; host home unchanged (tested) |
| Adoption UX | Developer with Docker runs `devcage agent` on a project in **&lt; 15 minutes** |
| Honesty | Start banner never claims allowlist when net is full container |

---

## 11. MVP scope

### MVP (v0.1) — agent wedge only

- Core profiles: **`agent`**, **`offline`**, **`host`**
- **`devcage agent`** happy path + `run` / `shell` / `stop --wipe` / `status` / `doctor`
- **Container backend only** (Docker/Podman)
- Project **RW** + ephemeral home + secret denylist
- Network: **none** (`offline`) enforced; **full container** for `agent` with honest label
- Optional RO named package caches
- Start banner + fail closed without runtime
- Docs: quickstart, agent recipes, residual risk

**Explicitly out of v0.1:** GUI/mobile profiles, allowlist net, bwrap, COW, custom profile DSL, required pkg-guard, durable cages, VM.

### Phase 2

- Real egress allowlist/categories when testable
- Linux `bwrap` fast-follow
- Optional `untrusted` profile
- Optional COW / wipe-oriented workspace
- Optional pkg-guard hooks
- Minimal `.devcage.toml`

### Phase 3 (appendix)

- GUI/mobile bridges — [workloads-gui-mobile.md](./workloads-gui-mobile.md)
- native-mac / VM backends as needed

---

## 12. Design decisions (closed)

Full detail: [architecture.md](./architecture.md#design-decisions-closed).

| # | Topic | Decision |
|---|--------|----------|
| P1 | Product job | Easy + secure **AI agent development** isolation |
| P2 | Core profiles | **`agent`**, **`offline`**, **`host`** only in v0.1 |
| P3 | Happy path | **`devcage agent`** |
| P4 | GUI/mobile | Phase 3 appendix; not core pitch |
| 1 | MVP backend | **Container-first** |
| 2 | Network | Enforce `none`; `agent` = full container **labeled**; allowlist later; no MITM |
| 3 | Default profile | **`agent`** |
| 4 | Workspace | RW project; COW later |
| 5 | Package caches | Optional RO named host caches |
| 6 | CLI name | **`devcage`** |

---

## 13. Out-of-repo dependencies / composition

| System | Role |
|--------|------|
| **pkg-guard** | Package name/version policy; shims for `uvx`/`npx`; OSV |
| **Docker/Podman/Colima** | **Primary MVP** isolation backend (Linux + macOS) |
| **bubblewrap** | Linux fast-follow backend |
| **AI CLIs** | Workloads: Grok, Claude Code, Gemini CLI, etc. |
| **Xcode / simctl** | Phase 3 only; hybrid host workflow until then |

DevCage **must not** require pkg-guard.

---

## 14. Documentation deliverables (product)

| Doc | Purpose |
|-----|---------|
| This PRD | Requirements and scope |
| Threat model | Residual risk (container honesty) |
| Agent recipes | Grok / Claude / Gemini via `devcage agent` |
| Comparison | vs vendor sandboxes, Dev Containers, plain Docker |
| [workloads-gui-mobile.md](./workloads-gui-mobile.md) | Phase 3 appendix only |

---

## 15. Naming

| Item | Value |
|------|--------|
| Product | **DevCage** |
| CLI | `devcage` |
| Config dir | `~/.config/devcage/` (or `$XDG_CONFIG_HOME/devcage`) |
| Project dir | `.devcage/` |

Rationale: clear “developer environment in a cage,” distinct from **pkg-guard** (packages) and any MCP-specific guards; short CLI; profile metaphor fits.

---

## 16. Revision history

| Version | Date | Notes |
|---------|------|--------|
| v0 | 2026-08-09 | Initial PRD from product discussion (isolation profiles for agents + package installs) |
| v0.1 | 2026-08-09 | Closed Section 12 design decisions: container-first MVP, phased network, RW workspace + later COW, RO package caches |
| v0.1.1 | 2026-08-09 | GUI/mobile workload design (later demoted to phase 3 appendix) |
| v0.2 | 2026-08-09 | **Agent wedge:** core profiles `agent`/`offline`/`host`; `devcage agent` happy path; GUI/mobile out of core; honest net labels |
