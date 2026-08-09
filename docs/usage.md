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
# with Docker/Podman:
devcage run -p offline -- echo hi          # no network
devcage run -p agent -- sh -c 'uname -a'   # full container net
devcage shell -p offline
devcage agent                              # runs $DEVCAGE_DEFAULT_AGENT in container image
devcage agent --offline claude
devcage stop --wipe                        # last/running session
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
| **`agent`** | container | full (honest label) | Default; needs image with your tools/agent |
| **`offline`** | container | **none** | Enforced via `--network none` |
| **`host`** | host process | unrestricted | Break-glass; ephemeral `$HOME` only |

## Commands

| Command | Behavior |
|---------|----------|
| `devcage doctor` | JSON: runtimes, image, data dir |
| `devcage status` | JSON session list |
| `devcage profile list\|show` | Built-in profiles |
| `devcage run -p <profile> -- <cmd…>` | Run command in cage |
| `devcage shell -p <profile>` | `/bin/sh -l` in cage |
| `devcage agent [name] [--offline]` | Happy path agent launch |
| `devcage stop [--wipe] [id]` | Kill container if any; wipe data |

## Configuration

| Variable | Purpose |
|----------|---------|
| `DEVCAGE_WORKSPACE` | Project path (default: cwd) |
| `DEVCAGE_DEFAULT_PROFILE` | Default for run/shell (`agent`) |
| `DEVCAGE_DEFAULT_AGENT` | Agent binary name in image (`claude`) |
| `DEVCAGE_CONTAINER_RUNTIME` | `docker` or `podman` |
| `DEVCAGE_CONTAINER_IMAGE` | Image (default Alpine 3.20) |
| `DEVCAGE_DATA_DIR` | Sessions + ephemeral homes |
| `DEVCAGE_SHARE_PACKAGE_CACHES` | `1` = RO host npm/uv/pip caches |
| `RUST_LOG` | Tracing |

### Important: macOS agents

Host-native agent binaries (Darwin) **do not** run inside a Linux container. Point `DEVCAGE_CONTAINER_IMAGE` at an image that **contains** the agent CLI (or use Linux tools only). Profile `host` runs the host binary with little isolation.

## Composition with pkg-guard (optional)

1. Host PATH: pkg-guard shims for `uvx` / `npx`
2. Workloads via `devcage run|agent`

## Quality gate

```bash
./scripts/check.sh
```
