# Usage

**Status:** MVP runtime — container profiles need Docker/Podman; host profiles always work.
**Primary agent:** **Grok Build** (default). Cursor and other CLIs are not first-class modes today.

| You want | Use |
|----------|-----|
| Daily coding with Grok (recommended) | **`devcage grok`** |
| Deep cage for one command | **`devcage run -p offline\|untrusted\|agent -- …`** |
| Architecture / threat model detail | [recommended-stack.md](./recommended-stack.md) |
| Container images | [agent-images.md](./agent-images.md) |

---

## Install (from source)

```bash
cp local.env.example local.env   # optional
./scripts/setup-hooks.sh
./scripts/build.sh --release
./scripts/install.sh             # ${PREFIX:-$HOME/.local}/bin/devcage
```

Ensure `devcage` and `grok` are on your `PATH`. Needs **Docker or Podman** (e.g. Colima on macOS) for `offline` / `untrusted` / container `agent`. `devcage doctor` reports availability.

---

## Working with Grok (full guide)

### Model in one sentence

**Keep Grok’s built-in tools; contain the process with Grok’s OS sandbox; use DevCage only for untrusted / offline / Linux commands.**
Do not put every edit behind MCP or inside a Linux agent image for day-to-day work.

```text
devcage grok
    │
    ├─ Grok --sandbox keys-first   (Seatbelt: write project + ~/.grok + tmp;
    │                               deny **/.env, **/*.pem, … via .grok/sandbox.toml)
    ├─ .grok/config.toml           (permission allow/deny for Bash/Read/Edit)
    ├─ DEVCAGE_DATA_DIR            (sandbox-safe session data for nested devcage)
    ├─ PATH includes this devcage  (so `devcage run` works inside Grok)
    │
    └─ optional deep cage:
         devcage run -p offline|untrusted|agent -- <cmd>
```

Complementary tools (not replaced by DevCage):

| Tool | Role |
|------|------|
| **mcp-secret-launcher** | Keys in Keychain → MCP process env only |
| **mcp-guard** | MCP tool policy / HITL |
| **pkg-guard** | `uvx` / `npx` supply-chain shims |

See [recommended-stack.md](./recommended-stack.md).

### Daily entrypoint: `devcage grok`

```bash
cd /path/to/your-project
devcage grok
```

What it does:

1. Resolves workspace (cwd or git root; override with `-w` / `DEVCAGE_WORKSPACE`).
2. **Seeds** project recipes if missing (unless `--no-init`):
   - `.grok/sandbox.toml` — profile `keys-first` (**extends Grok `strict`** + expanded secret `deny` + `restrict_network`)
   - `.grok/config.toml` — permission deny for secrets / `security` / break-glass host; allow offline/untrusted/agent deep cage
3. Sets child env:
   - `DEVCAGE_DATA_DIR` — sandbox-safe (`$TMPDIR/devcage-data`, not `~/.local/share`)
   - `DEVCAGE_MODE=grok-sandbox`
   - `DEVCAGE_GROK_SANDBOX=<profile>`
   - `DEVCAGE_DENY_HOST=1` — nested `devcage run -p host` **refused** (override with `--allow-host-profile`)
   - Stable `PATH` (`/usr/bin`, Homebrew, this binary, `~/.local/bin`, `~/.grok/bin`, …)
4. **Scrubs** ambient secrets from the Grok process: `SSH_AUTH_SOCK`, AWS secret keys, `GITHUB_TOKEN` / `GH_TOKEN`, OpenAI/Anthropic keys (keeps `XAI_API_KEY` if set).
5. Runs: **`grok --sandbox <profile> …`** (default profile `keys-first`).

After upgrading DevCage, re-seed recipes:

```bash
devcage init-grok --force
# or: devcage grok --force-init
```

Startup banner (stderr):

```text
devcage mode: grok-sandbox
  sandbox:   keys-first
  workspace: /path/to/project
  data_dir:  /var/folders/…/T/devcage-data   # or ~/.local/share/devcage
  deep cage: devcage run -p offline|untrusted|agent -- <cmd>
```

### `devcage grok` flags

| Flag / env | Default | Meaning |
|------------|---------|---------|
| `--sandbox <name>` | `keys-first` | Passed to `grok --sandbox`. Env: `DEVCAGE_GROK_SANDBOX` |
| `--strict` | off | Shortcut for `--sandbox strict` |
| `--no-init` | off | Do not write `.grok/` recipes |
| `--force-init` | off | Overwrite existing `.grok` recipes when seeding |
| `--allow-host-profile` | off | Sets `DEVCAGE_DENY_HOST=0` so nested `run -p host` is allowed |
| `--print` | off | Print plan (`mode`, `DEVCAGE_DATA_DIR`, scrub list, command); do **not** start Grok |
| `-w` / `--workspace` | cwd / git root | Project root for recipes and Grok cwd. Env: `DEVCAGE_WORKSPACE` |
| trailing args | — | Forwarded to Grok after `--sandbox <name>` |

Examples:

```bash
# Interactive Grok under keys-first sandbox
devcage grok

# Dry-run
devcage grok --print

# Built-in Grok profiles (no custom keys-first file required for these names)
devcage grok --sandbox workspace
devcage grok --sandbox strict
devcage grok --sandbox read-only

# Headless / single-turn Grok (args after --)
devcage grok -- -p "Summarize README.md" --always-approve

# Force re-seed recipes
devcage grok --force-init

# Only seed recipes (do not start Grok)
devcage init-grok
devcage init-grok --force
```

### Recipes under `.grok/`

| File | Purpose |
|------|---------|
| `.grok/sandbox.toml` | Custom profile **`keys-first`**: extends Grok `workspace`, kernel `deny` for secret globs (`**/.env`, `**/*.pem`, `**/.ssh/**`, …) |
| `.grok/config.toml` | Permission rules: deny secret paths / dangerous Bash; allow project tools + `Bash(devcage *)` |

Templates: [examples/grok/](../examples/grok/) (also embedded in the binary for `init-grok` / `grok` seed).

Tune denylists for your layout. Grok docs: sandbox + permissions under `~/.grok/docs/user-guide/` (`18-sandbox.md`, `22-permissions-and-safety.md`).

### Nested deep cage (from inside Grok)

While Grok is running via `devcage grok`, the agent (or you) can run:

```bash
devcage run -p offline -- ./sketchy-setup.sh
devcage run -p untrusted -- sh -c 'curl …'    # think twice
devcage run -p agent -- make test             # full container network
devcage doctor                                # check data_dir / data_dir_note
```

Because `DEVCAGE_DATA_DIR` and `PATH` are set, nested `devcage` should work without extra exports.

### Data directory under Grok sandbox

Grok `workspace` / `keys-first` typically allow writes to:

- project CWD
- `~/.grok/`
- `/tmp` (and macOS temp)

Default DevCage dir `~/.local/share/devcage` is often **blocked**.

**Automatic behavior:** if the default is not writable, DevCage write-probes and relocates to the first usable of:

1. `$TMPDIR/devcage-data`
2. `/tmp/devcage-data`
3. `./.devcage-data`

Prints a stderr note; `devcage doctor` exposes `data_dir` and `data_dir_note`.

Optional pin:

```bash
export DEVCAGE_DATA_DIR="${TMPDIR:-/tmp}/devcage-data"
```

Explicit `DEVCAGE_DATA_DIR` is never overridden by auto-fallback.

### Grok sandbox vs DevCage (what each does)

| Concern | Grok `--sandbox` (via `devcage grok`) | `devcage run` container |
|---------|----------------------------------------|-------------------------|
| All built-in Edit/Bash | **Limited** for whole session | Only if that command is run in the cage |
| Write only project (+ Grok state/tmp) | **Yes** | Per mount plan |
| Kernel deny `**/.env` etc. | **Yes** (recipe) | Don’t mount secrets; project still visible if mounted |
| Hard network none (macOS) | Weak / often no-op | **`offline` / `untrusted`** |
| Linux userland | No | **Yes** |
| Wipe disposable HOME | No (uses `~/.grok`) | **Yes** |
| Global skills / Grok auth | **Native** | N/A (Grok stays on host in this model) |

### Other Grok-related commands

| Command | When to use |
|---------|-------------|
| **`devcage grok`** | Daily: Grok + OS sandbox + DevCage-ready env |
| **`devcage init-grok`** | Seed `.grok` only |
| **`devcage agent`** | Run agent **inside** a cage profile (container or fallback) — not the daily Grok path |
| **`devcage agent --host-restricted`** | Host process + ephemeral HOME + env scrub + **auth seed** for `~/.grok/auth.json` |
| **`devcage agent --offline`** | Agent CLI under offline container profile (needs Linux agent in image) |

#### `devcage agent` vs `devcage grok`

| | `devcage grok` | `devcage agent` / `agent --host-restricted` |
|--|----------------|-----------------------------------------------|
| Goal | Daily Grok UX + sandbox | Put *an agent binary* in a DevCage session |
| Grok process | On host under Seatbelt | Optional; often wrong for Darwin Grok in Linux image |
| Built-ins | Full Grok tools | Depends on binary in venue |
| Auth | Normal `~/.grok` | `agent-host` seeds `auth.json` into ephemeral HOME |
| Recommended | **Yes** for daily | Deep / special cases |

#### Host-restricted agent notes (`agent-host`)

```bash
devcage agent --host-restricted
# or: devcage agent --host-restricted grok -- -p "…" --always-approve
```

- Profile **`agent-host`**: host backend, ephemeral `$HOME`, env scrub (cloud/SSH ambient; **keeps** `XAI_API_KEY`).
- Copies real home **`~/.grok/auth.json`** and config into the session home so Grok can sign in.
- **Not** a container; weaker than Grok Seatbelt for FS policy. Prefer **`devcage grok`** for daily isolation of built-ins.

Default agent name is **`grok`** (`DEVCAGE_DEFAULT_AGENT`).

### What is *not* supported as a first-class mode

- **Cursor CLI** — no `devcage cursor`. Use `devcage run` from Cursor’s shell if needed.
- **Grok entirely inside Alpine** — skills/auth pain; not the recommended path.
- **MCP proxy for every built-in tool** — fights Grok UX; use sandbox instead.

### Grok troubleshooting

| Symptom | Likely cause | Fix |
|---------|--------------|-----|
| `devcage error: Operation not permitted` under sandbox | Data dir under `~/.local/share` blocked, or stale `DEVCAGE_DATA_DIR` from an old `devcage grok` | Rebuild/install latest DevCage; **restart** with `devcage grok` (exports sandbox-safe `$TMPDIR/devcage-data`). This session: `export DEVCAGE_DATA_DIR=$TMPDIR/devcage-data` |
| `profile host is denied` | `DEVCAGE_DENY_HOST=1` (default under `devcage grok`) | Use `offline` / `untrusted` / `agent`, or `--allow-host-profile` / `DEVCAGE_DENY_HOST=0` |
| Still can read other repos / `~/.ssh` | Old `.grok/sandbox.toml` (extended `workspace`) | `devcage init-grok --force` then restart Grok (new recipe extends **`strict`**) |
| `SSH_AUTH_SOCK` still set | Old `devcage grok` binary | Install latest; restart session |
| Host HTTPS still works under sandbox | macOS Seatbelt limitation | Expected; use `devcage run -p offline` for no-net jobs |
| Keychain still reachable | Host process | Use mcp-secret-launcher + mcp-guard; sandbox cannot fully block Security.framework |
| `grok not found on PATH` | Grok not installed | Install Grok Build; check `PATH` |
| Sandbox profile `keys-first` unknown | Missing `.grok/sandbox.toml` | `devcage init-grok` or `devcage grok` (seeds by default) |
| Nested `devcage` not found | Old session without mode | Use `devcage grok` so PATH is fixed; or install `devcage` globally |
| Docker offline fails | Colima/Docker down | `colima start` / `devcage doctor` |
| Agent “not logged in” under `agent-host` | Auth not seeded | Ensure `~/.grok/auth.json` exists; use current DevCage with `auth_seed` |
| Want only Grok sandbox, no DevCage | — | `grok --sandbox keys-first` after `devcage init-grok` |

### Probe limits

```bash
# Inside a devcage grok session (or any shell):
devcage limits
```

JSON report: data dir, secret write deny, home read limits, host HTTPS note, `SSH_AUTH_SOCK` scrub, `DEVCAGE_DENY_HOST`, offline container echo + net block, host profile refusal.

### Headless example (CI-style smoke)

```bash
devcage grok --print
devcage grok --no-init -- -p "Reply with only: pong" --always-approve
```

---

## Happy path (all commands)

```bash
devcage doctor
devcage verify

# Daily Grok
devcage grok

# Deep cage
devcage pull
devcage run -p offline -- echo hi
devcage run -p untrusted -- sh
devcage run -p agent -- sh -c 'uname -a'
devcage shell -p offline
devcage agent --host-restricted
devcage export <session-id> out.txt ./out.txt
devcage stop --wipe
devcage clean
```

### Host break-glass (no real isolation)

```bash
devcage run -p host -- /usr/bin/true
```

## Start banner (session commands)

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
| **`agent-host`** | host | host | Host-restricted; ephemeral HOME + env scrub + auth seed |
| **`offline`** | container | **none** | Hardened; wipe by default |
| **`untrusted`** | container | **none** | Hardened; always wipe; no host caches |
| **`net-allowlist`** | container | full + advisory list | Sets `DEVCAGE_NET_ALLOWLIST` env (not kernel-enforced yet) |
| **`host`** | host process | unrestricted | Break-glass; ephemeral `$HOME` only |

## Commands

| Command | Behavior |
|---------|----------|
| `devcage doctor` | JSON: runtimes, image, `data_dir`, `data_dir_note`, readiness |
| `devcage status` | JSON session list |
| `devcage profile list\|show` | Built-in profiles |
| `devcage run -p <profile> -- <cmd…>` | Run command in cage |
| `devcage shell -p <profile>` | `/bin/sh -l` in cage |
| **`devcage grok […]`** | **Combined mode:** Grok `--sandbox` + DevCage-ready env (see above) |
| **`devcage init-grok`** | Seed `.grok/sandbox.toml` + `config.toml` |
| **`devcage limits`** | JSON isolation probes (sandbox + nested cage) |
| `devcage agent [name] [--offline] [--host-restricted]` | Agent in cage / agent-host |
| `devcage export <id> <src> <dest>` | Promote artifact from session |
| `devcage logs <id>` | Session event log (JSON lines) |
| `devcage init-allowlist [path]` | Write starter allowlist file |
| `devcage stop [--wipe] [id]` | Kill container if any; wipe data |
| `devcage verify` | Isolation smoke checks (JSON) |
| `devcage pull` | Pull configured container image |
| `devcage clean [--all]` | Wipe exited (or all) session dirs |
| `devcage explain -p <profile>` | Isolation plan without starting |

## Configuration

| Variable | Purpose |
|----------|---------|
| `DEVCAGE_WORKSPACE` | Project path (default: cwd / git root) |
| `DEVCAGE_DEFAULT_PROFILE` | Default for run/shell (`agent`) |
| `DEVCAGE_DEFAULT_AGENT` | Agent binary name (`grok` by default) |
| `DEVCAGE_GROK_SANDBOX` | Sandbox profile for `devcage grok` (`keys-first`) |
| `DEVCAGE_DENY_HOST` | `1` under `devcage grok`: refuse profile `host` unless `--allow-host-profile` |
| `DEVCAGE_CONTAINER_RUNTIME` | `docker` or `podman` |
| `DEVCAGE_CONTAINER_IMAGE` | Image (default Alpine 3.20) |
| `DEVCAGE_DATA_DIR` | Sessions + ephemeral homes (auto-fallback under sandbox) |
| `DEVCAGE_SHARE_PACKAGE_CACHES` | `1` = RO host npm/uv/pip caches |
| `DEVCAGE_AGENT_HOST_FALLBACK` | default on; `0` disables auto `agent-host` for `devcage agent` |
| `DEVCAGE_SEATBELT` | `1` = experimental Seatbelt for agent-host (macOS) |
| `DEVCAGE_NET_ALLOWLIST_FILE` | Path for net-allowlist profile |
| `RUST_LOG` | Tracing |

Mode markers set by `devcage grok` (for nested tools / debugging):

| Variable | Meaning |
|----------|---------|
| `DEVCAGE_MODE=grok-sandbox` | Combined mode is active |
| `DEVCAGE_GROK_SANDBOX` | Profile name passed to Grok |
| `DEVCAGE_DATA_DIR` | Session store for this tree of processes |
| `DEVCAGE_DENY_HOST` | `1` blocks break-glass `host` profile |

### macOS agents in containers

Host-native Grok (Darwin) **does not** run inside a Linux container image.

1. Daily: **`devcage grok`** (host Grok + sandbox).
2. Optional deep cage for **commands**: `devcage run`.
3. Only if you need the *agent binary* in a venue: Linux image ([agent-images.md](./agent-images.md)) or `agent --host-restricted`.
4. Profile `host` is full break-glass.

## Composition with pkg-guard / MCP (optional)

```text
Grok (via devcage grok)
  ├─ built-ins (sandboxed)
  ├─ MCP → mcp-guard → secret-launcher → MCP server
  ├─ PATH: pkg-guard shims (uvx/npx)
  └─ optional: devcage run …
```

1. Host PATH: pkg-guard shims for `uvx` / `npx`
2. MCP servers: wrap with mcp-guard + mcp-secret-launcher
3. Untrusted installs/scripts: prefer `devcage run -p offline|untrusted`

## Quality gate (contributors)

```bash
./scripts/check.sh
```

## Related docs

| Doc | Contents |
|-----|----------|
| [recommended-stack.md](./recommended-stack.md) | Keys-first architecture, composition tests |
| [examples/grok/](../examples/grok/) | Copy-paste sandbox + permission recipes |
| [agent-images.md](./agent-images.md) | Container images + host-restricted agents |
| [architecture.md](./architecture.md) | Backends and design decisions |
| [development.md](./development.md) | Build, lint, coverage |
