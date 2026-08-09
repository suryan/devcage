# Container images for DevCage agents

Isolated profiles (`agent`, `offline`, `untrusted`) run commands **inside a Linux container**. Host-native macOS binaries (many AI CLIs) **cannot** run there unless the image provides a Linux build.

## Defaults

| Variable | Default |
|----------|---------|
| `DEVCAGE_CONTAINER_IMAGE` | `docker.io/library/alpine:3.20` |

Alpine is fine for smoke tests (`echo`, `sh`). It is **not** a full agent environment.

## Recommended patterns

### 1. Linux tools only in the cage

```bash
export DEVCAGE_CONTAINER_IMAGE=docker.io/library/ubuntu:24.04
devcage pull
devcage run -p offline -- bash -lc 'apt-get update && apt-get install -y build-essential'
```

### 2. Agent binary inside a custom image

Build an image that installs your agent CLI (Node/Python/etc.) and set:

```bash
export DEVCAGE_CONTAINER_IMAGE=ghcr.io/you/devcage-agent:latest
devcage agent
```

Keep secrets out of the image; inject via env only when the profile allows network.

### 3. Host-restricted agent (no Docker / Darwin agents)

```bash
devcage agent --host-restricted
# or auto-fallback when Docker is missing (default):
# DEVCAGE_AGENT_HOST_FALLBACK=1  # default on
# DEVCAGE_AGENT_HOST_FALLBACK=0  # require container
```

`agent-host` provides:

- Ephemeral `$HOME` (session dir)
- Scrub of common secret env vars
- Project workspace as cwd

It does **not** provide a container boundary. Prefer container mode when possible.

## Offline / untrusted images

Prefer minimal images with no need for package installs on first boot:

- `docker.io/library/alpine:3.20`
- Distroless / scratch-based custom tool images

`untrusted` never shares host package caches.

## Related

- [usage.md](./usage.md)
- [architecture.md](./architecture.md)
