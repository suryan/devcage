# Usage

**Status:** Scaffold CLI only. Isolation is **not** enforced yet. Commands below describe the **intended** UX and what the binary does today.

**Product focus:** easy, secure isolation for **AI agent development** — not a general mobile/GUI sandbox platform.

## Install (from source)

```bash
cp local.env.example local.env   # optional
./scripts/build.sh --release
./scripts/install.sh             # ${PREFIX:-$HOME/.local}/bin/devcage
```

Ensure your preferred `bin` directory is on `PATH`.

## Happy path (intended)

```bash
# In a project directory (once isolation ships):
devcage doctor                 # need Docker/Podman
devcage agent                  # default profile=agent; launches default agent CLI
devcage agent claude           # or: grok, gemini — names from PATH / env
devcage agent --offline        # same FS isolation, network off
devcage stop --wipe            # end session; discard ephemeral home
```

On every start (intended banner):

```text
profile: agent
workspace: /path/to/project (rw)
home: ephemeral
network: full (container)    # or: none
secrets: not mounted
```

## Commands (scaffold)

| Command | Today | Intent |
|---------|--------|--------|
| `devcage doctor` | Probes `docker` / `podman` / `bwrap`; JSON | Environment health |
| `devcage status` | Empty session list | Active sessions |
| `devcage profile list` | Core + roadmap profiles (JSON) | Discover profiles |
| `devcage profile show <name>` | Placeholder JSON | Inspect one profile |
| `devcage agent [name]` | Errors: not implemented | **Happy path** — run agent in cage |
| `devcage run -p <profile> -- <cmd…>` | Errors: not implemented | Any command in cage |
| `devcage shell -p <profile>` | Errors: not implemented | Interactive shell in cage |
| `devcage stop [--wipe] [session]` | Errors: not implemented | Tear down session |

Examples today:

```bash
devcage doctor
devcage profile list

# Planned:
# devcage agent
# devcage agent --offline
# devcage run -p offline -- cargo test
# devcage stop --wipe
```

## Core profiles (v0.1)

| Profile | Network | FS | Intent |
|---------|---------|-----|--------|
| **`agent`** | Full container net (**labeled honestly**, not “allowlist”) | Project RW; ephemeral home; no host secrets | **Default** daily AI coding |
| **`offline`** | **None** (enforced) | Same FS isolation | Review / local-only without exfil |
| **`host`** | Unrestricted / near-host | Break-glass | Explicit escape hatch; loud warnings |

Optional later: `untrusted` for one-off package experiments (stricter, wipe-oriented).

**Mobile / GUI profiles** are phase 3 only — see [workloads-gui-mobile.md](./workloads-gui-mobile.md). Until then: cage the agent; keep Xcode / Studio / emulators on the host.

## Configuration

| Variable / file | Purpose |
|-----------------|---------|
| `local.env` | Local overrides (gitignored) |
| `DEVCAGE_WORKSPACE` | Workspace into the cage (also `--workspace`) |
| `DEVCAGE_DEFAULT_PROFILE` | Default profile (`agent`) |
| `DEVCAGE_DEFAULT_AGENT` | Default agent command name (`claude`, `grok`, …) |
| `DEVCAGE_BACKEND` | `auto` \| `container` (MVP); `bwrap` later |
| `RUST_LOG` | Tracing filter |

Optional minimal project config later: `.devcage.toml` with `default_profile` / `agent` only — not a full profile DSL in v0.1.

## Composition with pkg-guard (optional)

1. Host PATH: pkg-guard shims for `uvx` / `npx`
2. Start agents via `devcage agent`

DevCage does **not** require pkg-guard.

## Exit codes (scaffold)

| Code | Meaning |
|------|---------|
| 0 | Success (`doctor`, `status`, `profile`) |
| non-zero | Error, including “not implemented yet” for `agent` / `run` / `shell` / `stop` |
