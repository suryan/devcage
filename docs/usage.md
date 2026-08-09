# Usage

**Status:** MVP runtime — `agent` / `offline` use Docker or Podman; `host` is break-glass without container isolation.

## Install (from source)

```bash
cp local.env.example local.env   # optional
./scripts/setup-hooks.sh
./scripts/build.sh --release
./scripts/install.sh             # ${PREFIX:-$HOME/.local}/bin/devcage
```

Needs **Docker or Podman** for isolated profiles. `devcage doctor` reports availability.

## Happy path

```bash
devcage doctor
devcage verify                             # isolation smoke checks
devcage pull                               # pull DEVCAGE_CONTAINER_IMAGE
# with Docker/Podman:
devcage run -p offline -- echo hi          # no network; wipe by default
devcage run -p untrusted -- sh             # hardened + always wipe
devcage run -p agent -- sh -c 'uname -a'   # full container net
devcage shell -p offline
devcage agent                              # container, or agent-host fallback
devcage agent --host-restricted            # force host-restricted (Darwin agents)
devcage export <session-id> out.txt ./out.txt
devcage stop --wipe
devcage clean                              # wipe exited sessions
```

### Host break-glass (no real isolation)

```bash
devcage run -p host -- /usr/bin/true
```

## Start banner

```text
devcage session
  id:        dc-…
  profile:   offline
  backend:   docker
  image:     docker.io/library/alpine:3.20
  workspace: /path/to/project (rw)
  home:      ephemeral (…)
  network:   none (enforced)
  secrets:   not mounted
```

## Profiles

| Profile | Backend | Network | Notes |
|---------|---------|---------|--------|
| **`agent`** | container | full (honest label) | Default when Docker/Podman available |
| **`agent-host`** | host | host | Host-restricted; ephemeral HOME + env scrub |
| **`offline`** | container | **none** | Hardened; wipe by default |
| **`untrusted`** | container | **none** | Hardened; always wipe; no host caches |
| **`net-allowlist`** | container | full + advisory list | Sets `DEVCAGE_NET_ALLOWLIST` env (not kernel-enforced yet) |
| **`host`** | host process | unrestricted | Break-glass; ephemeral `$HOME` only |

## Commands

| Command | Behavior |
|---------|----------|
| `devcage doctor` | JSON: runtimes, image, data dir, readiness |
| `devcage status` | JSON session list |
| `devcage profile list\|show` | Built-in profiles |
| `devcage run -p <profile> -- <cmd…>` | Run command in cage |
| `devcage shell -p <profile>` | `/bin/sh -l` in cage |
| `devcage agent [name] [--offline] [--host-restricted]` | Happy path agent launch |
| `devcage export <id> <src> <dest>` | Promote artifact from session |
| `devcage logs <id>` | Session event log (JSON lines) |
| `devcage init-allowlist [path]` | Write starter allowlist file |
| `devcage stop [--wipe] [id]` | Kill container if any; wipe data |
| `devcage verify` | Isolation smoke checks (JSON) |
| `devcage pull` | Pull configured container image |
| `devcage clean [--all]` | Wipe exited (or all) session dirs |

## Configuration

| Variable | Purpose |
|----------|---------|
| `DEVCAGE_WORKSPACE` | Project path (default: cwd) |
| `DEVCAGE_DEFAULT_PROFILE` | Default for run/shell (`agent`) |
| `DEVCAGE_DEFAULT_AGENT` | Agent binary name (`grok` by default) |
| `DEVCAGE_CONTAINER_RUNTIME` | `docker` or `podman` |
| `DEVCAGE_CONTAINER_IMAGE` | Image (default Alpine 3.20) |
| `DEVCAGE_DATA_DIR` | Sessions + ephemeral homes |
| `DEVCAGE_SHARE_PACKAGE_CACHES` | `1` = RO host npm/uv/pip caches |
| `DEVCAGE_AGENT_HOST_FALLBACK` | default on; `0` disables auto `agent-host` |
| `RUST_LOG` | Tracing |

### Important: macOS agents

Host-native agent binaries (Darwin) **do not** run inside a Linux container.

1. Prefer a Linux image that ships the agent — see [agent-images.md](./agent-images.md).
2. Or use **`devcage agent --host-restricted`** / auto-fallback (`agent-host`).
3. Profile `host` is full break-glass.

## Composition with pkg-guard (optional)

1. Host PATH: pkg-guard shims for `uvx` / `npx`
2. Workloads via `devcage run|agent`

## Quality gate

```bash
./scripts/check.sh
```
