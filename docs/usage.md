# Usage

**Status:** MVP runtime — container profiles need Docker/Podman; host profiles always work.
**Agents:** **any CLI.** Grok is the default *name* and has a dedicated command; Claude / Cursor / Codex use **`devcage wrap`**.

| You want | Use |
|----------|-----|
| Daily coding with **Grok** (recommended) | **`devcage grok`** |
| **Any other agent** (Claude, Cursor, Codex, …) without its own sandbox | **`devcage wrap -- <agent…>`** |
| Box **one MCP connector** (agent stays on the host) | **`devcage mcp -- <command…>`** |
| Deep cage for one command | **`devcage run -p offline\|untrusted\|agent -- …`** |
| Architecture / threat model detail | [recommended-stack.md](./recommended-stack.md) |
| Container images | [agent-images.md](./agent-images.md) |

---

## Install (from source)

One-liner (clones `main`, bootstraps rustup if needed, installs to `$HOME/.local/bin`):

```bash
curl -fsSL https://raw.githubusercontent.com/suryan/devcage/main/scripts/install.sh \
  | bash -s -- --yes
```

Pin a ref / prefix:

```bash
curl -fsSL https://raw.githubusercontent.com/suryan/devcage/main/scripts/install.sh \
  | bash -s -- --yes --ref main --prefix "$HOME/.local"
```

From a local clone:

```bash
cp local.env.example local.env   # optional
./scripts/setup-hooks.sh
./scripts/build.sh --release
./scripts/install.sh             # ${PREFIX:-$HOME/.local}/bin/devcage
```

Ensure `devcage` and `grok` are on your `PATH`. Needs **Docker or Podman** (e.g. Colima on macOS) for `offline` / `untrusted` / container `agent`. `devcage doctor` reports availability.

---

## Working with Grok (full guide)

This section is the operator guide for **daily Grok + optional DevCage deep cage**.
Read it as: *what to type*, then *what that step actually means*.

### Big picture (two layers)

| Layer | What it is | What it is **for** |
|-------|------------|---------------------|
| **Grok OS sandbox** (`devcage grok` → `grok --sandbox keys-first`) | Kernel Seatbelt on the **whole Grok process** (edits, bash, tools) | Stop the agent from trashing the disk / reading other projects / touching secret *paths* while you still use normal Grok skills |
| **DevCage deep cage** (`devcage run -p offline\|untrusted\|agent`) | Separate **Linux container** (or rare host profile) for **one command** | Untrusted scripts, hard no-network, Linux-like runs — stronger than Seatbelt for that job |

**Rule of thumb:** edit the project with Grok under sandbox. Cage only the scary commands.

```text
You run:  devcage grok
              │
              ▼
         Grok (host) + skills + built-in tools
         limited by Seatbelt + .grok recipes
              │
              │  when needed:
              ▼
         devcage run -p offline -- ./sketchy.sh
              │
              ▼
         Linux container: project mounted, no host home, network none
```

Complementary tools (still your key/package story — DevCage does not replace them):

| Tool | Meaning |
|------|---------|
| **mcp-secret-launcher** | MCP API keys live in **Keychain**, not in `mcp.json` |
| **mcp-guard** | Policy / HITL on **MCP tool calls** |
| **pkg-guard** | Policy on **package installs** (`uvx` / `npx` shims) |

Deeper architecture: [recommended-stack.md](./recommended-stack.md).

---

### First-time setup (do this once per machine)

Each step has a purpose — do not skip install if you care about nested Docker.

| Step | Command | What it means |
|------|---------|----------------|
| **1. Build / install `devcage`** | `curl …/install.sh \| bash -s -- --yes` or `./scripts/install.sh` | Puts `devcage` on your PATH so both you and nested Grok shells can run it |
| **2. Ensure Grok is installed** | `which grok` | Combined mode launches **host** Grok, not a container image |
| **3. Container runtime (for deep cage)** | Colima/Docker or Podman; `devcage doctor` | Without this, `run -p offline` cannot start a Linux cage |
| **4. Pull image (optional but useful)** | `devcage pull` | Downloads Alpine (default) so first offline run is faster |
| **5. Seed project recipes** | `cd your-project && devcage init-grok` | Writes `.grok/sandbox.toml` + `.grok/config.toml` into **this repo** |

**What “seed recipes” means:**
DevCage writes project-local config that Grok loads when you use `--sandbox keys-first`:

| File | Layer | Meaning |
|------|--------|---------|
| `.grok/sandbox.toml` | **Kernel** (Seatbelt / Linux Landlock) | Profile `keys-first`: extends Grok **`strict`** (mostly project-only reads), secret path `deny` (exact `.env` + globs), and **directory-only** `read_only` tool paths (Homebrew, `~/.local/bin`, Colima, …). Child network is **on** so nested Docker can reach `docker.sock`. |
| `.grok/config.toml` | **Model policy** | Soft rules: deny Keychain/`security`, host break-glass bash, secret paths; allow cargo/git and safe `devcage run -p offline\|…` |

After **upgrading** DevCage, re-seed so recipes stay current:

```bash
devcage init-grok --force
```

**Why `--force`:** without it, existing `.grok/*` files are left alone (so old broken recipes can stick around).

---

### Daily workflow (what to type every day)

#### Step A — Start the combined session

```bash
cd /path/to/your-project
devcage grok
```

**What this means (in order):**

1. **Find the project root**
   Uses current directory or git root (`-w` / `DEVCAGE_WORKSPACE` to override). Grok’s cwd and recipe paths are this project.

2. **Seed recipes if missing**
   Same as `init-grok` unless you pass `--no-init`. Does not overwrite unless `--force-init`.

3. **Pick a sandbox-safe DevCage data directory**
   Session metadata / ephemeral homes go under something like `$TMPDIR/devcage-data`, **not** `~/.local/share/devcage` (Seatbelt often blocks that).
   Exported as `DEVCAGE_DATA_DIR` so nested `devcage run` writes somewhere the sandbox allows.

4. **Prepare a safer Grok process environment**
   - `DEVCAGE_MODE=grok-sandbox` — marks “we’re in combined mode”
   - `DEVCAGE_DENY_HOST=1` — nested **`devcage run -p host` is refused** (break-glass blocked by default)
   - **Scrub** ambient secrets from the child: `SSH_AUTH_SOCK`, AWS secret keys, GitHub/OpenAI/Anthropic tokens (**keeps** `XAI_API_KEY` if set so Grok can still auth)
   - **Stable PATH** — system bins + Homebrew + this `devcage` + `~/.local/bin` + `~/.grok/bin` so tools aren’t “not found” under sandbox

5. **Start Grok under Seatbelt**
   Effectively: `grok --sandbox keys-first` (or another profile you pass).
   From here, **every** Grok built-in tool (edit, bash, …) runs under that kernel policy.

**Banner fields (stderr) — what they mean:**

```text
devcage mode: grok-sandbox
  sandbox:   keys-first          ← Grok Seatbelt profile name
  workspace: /path/to/project    ← project root
  data_dir:  …/T/devcage-data    ← where nested DevCage sessions store data
  deny_host: 1                   ← host profile blocked
  scrub:     SSH_AUTH_SOCK, …    ← env vars removed from Grok
  deep cage: devcage run -p …    ← how to go deeper when needed
  limits:    devcage limits      ← how to self-test isolation
```

**Dry-run (see plan without starting Grok):**

```bash
devcage grok --print
```

#### Step B — Normal coding inside Grok

Use Grok as usual (chat, skills, edit files, run tests).

| You expect | Meaning |
|------------|---------|
| Can edit project files | Seatbelt allows writes under the project (+ `~/.grok` / temp) |
| Cannot write Desktop / random home paths | Kernel write scope |
| Cannot casually read other repos / `~/.ssh` | `strict`-based profile + deny globs |
| Cannot create/read `**/.env` style secrets in tree | Kernel `deny` in `.grok/sandbox.toml` |
| Host HTTPS / child `curl` may still work | `keys-first` leaves child network on so Docker CLI can talk to `docker.sock`. Hard no-net is `devcage run -p offline`. macOS Seatbelt net limits are also weak. |

#### Step C — Deep cage only when needed

Still inside the same Grok session (or any shell that inherited env):

```bash
devcage run -p offline -- ./sketchy-setup.sh   # no network, disposable
devcage run -p untrusted -- sh                 # stricter offline-style
devcage run -p agent -- make test              # full container network
devcage limits                                 # self-test (JSON)
```

**What each profile means here:**

| Profile | Meaning |
|---------|---------|
| **`offline`** | Linux container, **network=none**, hardened, wipe by default — use for untrusted / “no net” jobs |
| **`untrusted`** | Like offline but stricter (always wipe, no host package caches) |
| **`agent`** | Linux container with **full container network** (honest label — not an allowlist) |
| **`host`** | **Denied** under `devcage grok` by default — almost no isolation |

**Why nested run works without extra exports:**
`devcage grok` already set `DEVCAGE_DATA_DIR` and a usable `PATH`. The child `devcage` finds Docker (via allowed Homebrew paths in the sandbox recipe) and writes sessions under the temp data dir.

#### Step D — When you upgrade DevCage

```bash
# outside sandbox (normal terminal)
cargo build --release && cp target/release/devcage ~/.local/bin/
devcage init-grok --force    # refresh .grok recipes
devcage grok                 # new session picks up new binary + recipes
```

**Meaning:** old binaries may export a bad data dir or list files in `read_only`; old recipes may extend `workspace` (too open) or include illegal file paths.

---

### What each important command means

| Command | Meaning in one line |
|---------|---------------------|
| **`devcage grok`** | “Start Grok in combined mode: Seatbelt + recipes + safe env for nested DevCage.” |
| **`devcage init-grok`** | “Only write/update `.grok` recipes; do not start Grok.” |
| **`devcage init-grok --force`** | “Overwrite recipes with the current embedded defaults (absolute tool **directories**).” |
| **`devcage grok --print`** | “Show env + command that would run; exit without Grok.” |
| **`devcage grok --force-init`** | “Re-seed recipes, then start Grok.” |
| **`devcage grok --strict`** | “Use Grok’s built-in `strict` profile name instead of `keys-first`.” |
| **`devcage grok --allow-host-profile`** | “Allow nested `run -p host` (sets `DEVCAGE_DENY_HOST=0`) — rarely needed.” |
| **`devcage limits`** | “Run isolation probes and print JSON (data dir, secret deny, home limits, offline net, host deny).” |
| **`devcage doctor`** | “Is Docker/Podman ready? Where is data_dir?” |
| **`devcage run -p offline -- …`** | “Run this command in a disposable no-net Linux cage.” |
| **`devcage agent`** | “Run an agent *binary inside* a cage” — **not** the daily Grok path (use `devcage grok` instead). |

### `devcage grok` flags (reference)

| Flag / env | Default | Meaning |
|------------|---------|---------|
| `--sandbox <name>` | `keys-first` | Name passed to `grok --sandbox` (must exist in Grok built-ins or `.grok/sandbox.toml`) |
| `--strict` | off | Shortcut for `--sandbox strict` |
| `--no-init` | off | Do not write `.grok/` files |
| `--force-init` | off | Overwrite existing recipes |
| `--allow-host-profile` | off | Allow break-glass host profile for nested DevCage |
| `--print` | off | Dry-run plan only |
| `-w` / `--workspace` | cwd / git root | Project root |
| trailing args | — | Passed to Grok (e.g. `-p "prompt" --always-approve`) |

Examples:

```bash
devcage grok
devcage grok --print
devcage grok -- -p "Summarize README.md" --always-approve
devcage init-grok --force
```

---

### Recipes under `.grok/` (what they control)

| File | Who enforces it | Meaning |
|------|-----------------|---------|
| **`.grok/sandbox.toml`** | **Kernel** (Grok Seatbelt / Landlock) | Hard FS limits. `keys-first` extends **`strict`**, denies exact `.env`/`local.env` plus secret globs, opens **directories only** for tools (`/opt/homebrew`, `~/.local/bin`, `~/.colima`, …). **`restrict_network` is off** so Linux nested Docker can use `docker.sock`. **Files** in `read_only` (e.g. `~/.gitconfig`) make Grok refuse to start. |
| **`.grok/config.toml`** | **Grok permission engine** | Soft policy on tool calls (deny `security`, host break-glass bash; allow cargo + safe `devcage` forms). Complements but does not replace Seatbelt. |

Generated by `init-grok` / first `devcage grok`. Absolute tool paths are filled for **this machine’s HOME**.

Upstream Grok docs: `~/.grok/docs/user-guide/18-sandbox.md`, `22-permissions-and-safety.md`.

---

### Data directory (`DEVCAGE_DATA_DIR`) — what it means

DevCage stores session ids, ephemeral homes, and event logs under **data_dir**.

| Situation | Meaning |
|-----------|---------|
| Under `devcage grok` | Forced to a **sandbox-writable** path (`$TMPDIR/devcage-data`, …) so nested runs do not hit `Operation not permitted` on `~/.local/share` |
| Auto-fallback (any run) | If the configured dir is not writable, try `$TMPDIR` → `/tmp` → `./.devcage-data` and print a note |
| Explicit export | You set `DEVCAGE_DATA_DIR=…`; if unusable under sandbox, DevCage still tries sandbox-safe fallback |

```bash
# optional pin (usually unnecessary with current devcage grok)
export DEVCAGE_DATA_DIR="${TMPDIR:-/tmp}/devcage-data"
```

---

### Grok sandbox vs DevCage (compare)

| Concern | Grok sandbox (`devcage grok`) | Deep cage (`devcage run`) |
|---------|-------------------------------|---------------------------|
| Applies to all Grok edits/bash | **Yes** | Only that command |
| Limit writes to project | **Yes** | Mount policy |
| Deny secret path globs | **Yes** (kernel; Linux exact `.env` + globs) | Don’t mount secrets |
| Hard no network | **No** — child net stays on so Docker works | **`offline` / `untrusted`** |
| Linux userland | No | **Yes** |
| Wipe disposable HOME | No | **Yes** |
| Global skills / auth | **Native host** | N/A |

---

### `devcage agent` vs `devcage grok` (do not confuse)

| | **`devcage grok`** | **`devcage agent` / `--host-restricted`** |
|--|--------------------|-------------------------------------------|
| Purpose | Daily coding with isolation on **built-ins** | Run an agent **inside** a DevCage session |
| Process | Host Grok + Seatbelt | Agent binary in container or agent-host |
| Skills / auth | Normal | agent-host seeds `~/.grok/auth.json` into ephemeral HOME |
| Recommended | **Yes** | Special cases only |

```bash
# Special case — not daily:
devcage agent --host-restricted
```

---

### Probe isolation: `devcage limits`

```bash
# Best inside a devcage grok session:
devcage limits
```

**What it means:** runs automated checks and prints JSON:

- Can we write data_dir / project?
- Is `.env` write blocked? Outside project write blocked?
- Are sibling home paths limited?
- Is host HTTPS reachable? (informational on macOS)
- Is `SSH_AUTH_SOCK` scrubbed in grok mode?
- Is `DEVCAGE_DENY_HOST` armed?
- Does offline container work and block network?

`pass: true` means **critical** checks succeeded (not every informational flag).

---

### Generic wrap: any agent without its own sandbox

Agents that **do not** implement `--sandbox` (Claude Code, Cursor CLI, Codex, custom CLIs) can still be process-isolated by DevCage:

```bash
devcage wrap -- claude
devcage wrap -- cursor-agent
devcage wrap -- codex
devcage wrap --print -- /usr/bin/true    # dry-run
devcage wrap --net none -- claude        # Seatbelt denies process network (macOS)
devcage wrap --no-seatbelt -- my-agent   # env wiring only
devcage wrap --ephemeral-home -- claude  # remap HOME (may break login)
```

**What `devcage wrap` means:**

| Piece | Meaning |
|-------|---------|
| **macOS Seatbelt** | `sandbox-exec` around the process. **Reads:** system + project + named tool/agent dirs (`~/.claude`, `~/.cursor`, `~/.grok`, Homebrew, …) — **not the whole home**. **Writes:** project, tmp, data_dir, those agent dirs. **`--net none`** omits `allow network*`. |
| **Env scrub** | Same ambient scrub as `devcage grok` (`SSH_AUTH_SOCK`, AWS secrets, GH/OpenAI/Anthropic tokens) |
| **`DEVCAGE_DATA_DIR`** | Sandbox-safe path for nested `devcage run` |
| **`DEVCAGE_DENY_HOST=1`** | Nested break-glass `host` profile blocked by default |
| **Real `HOME` (default)** | Agent auth/skills keep working; optional `--ephemeral-home` for stricter (often breaks OAuth) |
| **Linux** | Uses **`bwrap`** when on `PATH` (same binds: project/data/agent dirs; `--unshare-net` if `--net none`). If `/etc/resolv.conf` is a symlink, the resolved file (for example `stub-resolv.conf`) is bind-mounted read-only so DNS works. `devcage mcp` uses the same bind. If `bwrap` is missing → env-only + warning |

**`devcage grok` vs `devcage wrap`:**

| | `devcage grok` | `devcage wrap` |
|--|----------------|----------------|
| Agent | Grok only | **Any** command |
| Sandbox source | Grok’s own `--sandbox` + `.grok` recipes | **DevCage Seatbelt** (or env-only) |
| Best when | You use Grok daily | Agent has no sandbox mode |

Nested deep cage is the same for both:

```bash
# inside wrapped agent shell tools, or any shell:
devcage run -p offline -- ./sketchy.sh
```

### What is *not* supported as a first-class mode

| Not supported | Meaning |
|---------------|---------|
| **Named `devcage cursor` / `devcage claude` aliases** | Use `devcage wrap -- …` (same isolation) |
| **Grok inside Alpine daily** | Skills/auth break; use host Grok + sandbox instead |
| **MCP as replacement for all built-ins** | Model ignores dual APIs; contain the process instead |
| **Full Keychain lockout via sandbox alone** | Keep mcp-secret-launcher + mcp-guard |
| **Linux wrap without `bwrap`** | Env-only; install bubblewrap for real namespaces |

---

### Troubleshooting (symptom → meaning → fix)

| Symptom | What it means | Fix |
|---------|----------------|-----|
| `Could not resolve host` inside Linux `wrap` or `mcp` | `/etc/resolv.conf` points at a file on `/run` (systemd-resolved `stub-resolv.conf`) that the cage did not mount | Current DevCage bind-mounts that resolved file read-only |
| `npx: not found` inside Linux `devcage mcp` (nvm) | `bin/npx` points at `../lib/node_modules/...` and only `bin` was mounted | Current DevCage bind-mounts that Node prefix read-only |
| `Expected a directory but got a file` (e.g. `.gitconfig`) | `read_only` listed a **file**; Grok requires **directories** | Latest DevCage + `devcage init-grok --force` |
| `profile host is denied` | Combined mode blocks break-glass host | Use `offline`/`untrusted`/`agent`, or `--allow-host-profile` |
| `Operation not permitted` on data dir | Session data under blocked home path | Use current `devcage grok` (temp data_dir) or export `DEVCAGE_DATA_DIR=$TMPDIR/devcage-data` |
| Docker / Homebrew “not permitted” under sandbox | `strict` without tool **directories** in recipe | `init-grok --force` so `/opt/homebrew`, `~/.colima`, `~/.local/bin` are listed |
| Docker `connect: operation not permitted` on `docker.sock` | Old `keys-first` set `restrict_network=true`; Linux seccomp blocks unix `connect()` | Latest DevCage + `devcage init-grok --force` (generated recipes set `restrict_network=false`); restart `devcage grok` |
| Nested offline: no container runtime | Docker not on PATH or Colima down, or recipe blocks tool paths | `colima start`; re-seed recipes; restart `devcage grok` |
| Host HTTPS / child `curl` still works | `keys-first` does not restrict child network (needed for Docker) | Use `devcage run -p offline` for no-net jobs |
| `.env` write still allowed under `devcage limits` | Linux deny **globs** only match files that exist at launch | Generated recipe exact-denies `.env` / `local.env`; re-seed and restart |
| Keychain still usable | Host process can call Security APIs | mcp-secret-launcher + mcp-guard; don’t put secrets in project |
| Can still read other repos | Old recipe extended `workspace` | Re-seed (`extends = strict`) + restart Grok |
| `SSH_AUTH_SOCK` still set | Old binary did not scrub | Install latest; restart via `devcage grok` |
| `keys-first` unknown | Recipe missing | `devcage init-grok` or plain `devcage grok` |
| Agent “not logged in” on agent-host | Ephemeral HOME without auth seed | Prefer `devcage grok`; or ensure `~/.grok/auth.json` + current DevCage |

---

### Headless smoke (optional)

```bash
devcage grok --print
devcage grok --no-init -- -p "Reply with only: pong" --always-approve
```

**Meaning:** `--print` checks wiring; `-p` is Grok single-turn mode for automation.

---

## Happy path (command checklist)

```bash
devcage doctor          # runtime ready?
devcage verify          # basic isolation smoke
devcage init-grok       # first time / after upgrade use --force
devcage grok            # daily session

# inside Grok or a nested shell:
devcage limits
devcage run -p offline -- echo hi
devcage pull            # once, for image
```

### Host break-glass (no real isolation)

Only outside combined mode, or with `--allow-host-profile`:

```bash
devcage run -p host --allow-host-profile -- /usr/bin/true
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
| **`devcage wrap -- <cmd…>`** | Any agent under process wrap. Flags: `--net none`, `--print`, `--no-seatbelt`, `--ephemeral-home` |
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
Grok (via devcage grok)          # optional; the agent is not required to be caged
  ├─ built-ins
  ├─ MCP client spawns, per server you choose:
  │    mcp-guard → mcp-secret-launcher → devcage mcp → uvx|npx → that connector
  ├─ PATH: pkg-guard shims (uvx/npx)
  └─ optional: devcage run …
```

`devcage mcp` is connector isolation only. Cursor, Grok, Claude, or any other client keeps running on the host. You do not need `devcage grok`, `devcage wrap`, or `devcage run` for the connector cage. Connectors you leave out of the chain launch as they do today.

That instance is limited to the secret injected for its profile, the network you leave on (`--net allowed` by default, `--net none` to block), a workspace only with `--share-workspace ro|rw`, and an ephemeral home that is wiped on exit. It does not get Keychain, the SSH agent, or the rest of the host home. One profile's credential is a much smaller exposure than every secret on the instance.

```bash
devcage mcp -- uvx my-server==1.2.3
devcage mcp --net none --share-workspace ro -- npx -y some-server@1.0.0
devcage mcp --print -- uvx my-server==1.2.3
devcage mcp --container -- uvx my-server==1.2.3   # Linux guest; see note below
```

`--container` on macOS runs a Linux guest. A Darwin `uvx` / `npx` binary cannot run there. Use the default light wrap for host-native connectors.

On Linux, an nvm-style Node prefix is mounted read-only when `bin/npx` or `bin/npm` is a symlink into sibling `lib`. The host home and credential directories (`~/.aws`, `~/.ssh`, Keychains) are not mounted.

Light wrap fails closed when `sandbox-exec` and `bwrap` are both missing. `--allow-env-only` starts anyway and does not limit the filesystem. Apple platform binaries can still reach Keychain; ordinary connector binaries (uv, node, python) cannot.

Copy-paste chain: [examples/mcp/](../examples/mcp/).

1. Host PATH: pkg-guard shims for `uvx` / `npx` (they stay the command inside `devcage mcp`)
2. MCP servers you choose to box: mcp-guard, then mcp-secret-launcher, then `devcage mcp`
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
