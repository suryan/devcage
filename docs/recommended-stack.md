# Recommended stack: Grok built-ins + process containment

**Status:** product guidance (preferred daily model for Grok users).

This is the architecture DevCage **recommends** when the primary agent is **Grok Build** and you care about **keys** more than re-homing the entire agent into a container.

## Principle

> **Keep built-ins; contain the process, not the protocol.**

Do **not** put every file edit and shell command behind an MCP “container proxy.” Grok’s strength is its native tools (`read` / `edit` / `bash` / `grep`, skills, subagents). A second API the model can ignore loses isolation **or** effectiveness.

Instead:

1. Run **Grok on the host** (global skills, auth, Keychain-adjacent MCP launchers).
2. Constrain what those **same** tools can touch via **OS sandbox + permission rules**.
3. Keep **secrets** on the existing MCP / Keychain path.
4. Use **DevCage containers only** for explicit untrusted / offline / Linux jobs.

```text
Grok (host) + global skills + built-in tools
    │
    ├─ Grok --sandbox keys-first         (via `devcage grok`)
    ├─ Project permission deny           (Read/Edit/Bash rules)
    ├─ MCP → mcp-guard → secret-launcher → MCP servers   (keys)
    ├─ pkg-guard shims                   (uvx / npx supply chain)
    └─ optional: Bash allowlist includes `devcage run …`
         for truly untrusted / offline / Linux-only work

Other agents (no native sandbox):
    devcage wrap [--net none] -- claude|cursor-agent|codex
```

## Layer map

| Layer | What it protects | What it does **not** do |
|-------|------------------|-------------------------|
| **Grok `--sandbox`** | Kernel FS write scope; optional path deny (`.env`, `*.pem`, secret dirs) | Not a Linux container; not MCP policy |
| **Permission deny / allow** | Which tools/commands the **model is allowed to call** | Not kernel enforcement alone |
| **mcp-secret-launcher** | Keys stay in Keychain; injected only into MCP server env | Does not sandbox Grok’s bash |
| **mcp-guard** | MCP tool allow/deny, args, HITL | Does not constrain built-in `Edit`/`Bash` |
| **pkg-guard** | Package install policy on shims | Runtime secret isolation |
| **DevCage container** | Stronger venue for **commands** you deliberately cage | Not the daily I/O path for every keystroke |
| **`devcage wrap`** | Process Seatbelt (macOS) or **bwrap** (Linux) around **any** agent; `--net none`; no wholesale `$HOME` read | Not Grok’s `--sandbox`; env-only if neither tool is present |

## Threat model fit

| You care about | How this stack answers |
|----------------|------------------------|
| Keys (even non-prod) | Keychain + secret-launcher; sandbox `deny` for `**/.env`, `**/*.pem`; no host secret mounts into casual shells |
| Keychain / SSH | Stay on host; never mount into DevCage; scrub `SSH_AUTH_SOCK` in host-restricted profiles |
| MCP abuse | mcp-guard policies |
| Malicious packages | pkg-guard |
| Project code burnable | Sandbox may still write the project — acceptable if keys are out of tree |
| Untrusted one-shot command | `devcage run -p offline\|untrusted -- …` |

## Why not “Grok → MCP → container” for everything?

- Built-ins stay preferred by the model; MCP becomes optional.
- Re-implementing Edit/Bash as MCP tools hurts skills and latency.
- Denying all built-ins forces a second agent surface you must maintain.

**Process containment** applies policy to the tools Grok already uses.

## Daily workflow (Grok)

**Preferred entrypoint** — one mode that wires both layers:

```bash
# From the project root
devcage grok
# → seeds .grok recipes if missing
# → sets DEVCAGE_DATA_DIR (sandbox-safe / auto-relocated)
# → prepends this devcage binary on PATH
# → runs: grok --sandbox keys-first …
```

Equivalents / options:

```bash
devcage grok --sandbox workspace
devcage grok --sandbox strict
devcage grok --print              # dry-run: show env + command
devcage init-grok                 # only write .grok/ recipes
# manual (without the wrapper):
grok --sandbox keys-first

# Any other agent (no native sandbox):
devcage wrap -- claude
devcage wrap --net none -- cursor-agent
```

Optional project defaults (committed recipes under [examples/grok/](../examples/grok/)):

| File | Role |
|------|------|
| `.grok/sandbox.toml` | `keys-first` extends **`strict`**, exact `.env` deny + globs; child network left on so Linux Docker works |
| `.grok/config.toml` | Permission deny secrets/`security`/host break-glass; allow offline/untrusted deep cage |
| Project skill / rule | “Untrusted installs and offline jobs go through `devcage run`” |

### Sandbox (kernel)

Copy and adjust [examples/grok/sandbox.toml](../examples/grok/sandbox.toml):

```bash
mkdir -p .grok
cp examples/grok/sandbox.toml .grok/sandbox.toml
grok --sandbox keys-first
```

### Permissions (model policy)

Copy and adjust [examples/grok/config.toml](../examples/grok/config.toml):

```bash
cp examples/grok/config.toml .grok/config.toml
```

Tune `deny` / `allow` for your repo. `deny` always wins over always-approve.

### Secrets (unchanged)

```text
mcp-guard → mcp-secret-launcher → uvx|npx MCP server
```

Do **not** put API tokens in project `.env` if you can avoid it; if they must exist on disk, list them in sandbox `deny`.

### Packages

Keep pkg-guard shims first on `PATH` for `uvx` / `npx` on the host (and inside images when you use DevCage for installs).

## When to use DevCage

| Use DevCage | Prefer host Grok + sandbox only |
|-------------|----------------------------------|
| Offline / no-network command runs | Normal editing, review, refactors |
| Untrusted tarball / sketchy script | Trusted project day-to-day work |
| Need Linux userland (not Darwin) | macOS-native tools and Grok skills |
| Disposable shell you will wipe | Keys and MCP already isolated |

```bash
# IMPORTANT when Grok --sandbox is on (see composition below):
export DEVCAGE_DATA_DIR="${TMPDIR:-/tmp}/devcage-data"

devcage run -p offline -- ./sketchy-setup.sh
devcage run -p untrusted -- sh -c 'curl …'   # still think twice
devcage run -p agent -- make test            # full container net, labeled
```

Optional permission shape: allow `Bash(devcage *)` for caged jobs; deny broad destructive host patterns.

## Do they work together? (tested)

**Yes.** They are independent processes that compose. DevCage **auto-relocates** its data dir when the default is not writable under Grok sandbox.

| Component | Tested how | Status |
|-----------|------------|--------|
| DevCage unit/CLI/container e2e | `cargo test`, Colima offline net | **Yes** (in-repo) |
| Grok host-restricted via DevCage `agent-host` | Real headless Grok create file + auth seed | **Yes** (manual) |
| Grok `--sandbox keys-first` **alone** | Grok product feature | Upstream Grok; not DevCage CI |
| **Grok sandbox + `devcage run` together** | Manual smoke on macOS + Colima; auto data-dir fallback | **Yes** (with auto-fallback) |
| Full stack mcp-guard + secret-launcher + pkg-guard + sandbox + DevCage in one session | End-to-end automation | **Not in DevCage CI** (compose yourself) |

### Data dir under Grok sandbox

Grok `workspace` / `keys-first` typically allows writes to project CWD, `~/.grok/`, and `/tmp`. Default **`~/.local/share/devcage`** is often blocked.

**Automatic fix (current DevCage):** write-probe the default; if it fails, use the first writable of:

1. `$TMPDIR/devcage-data`
2. `/tmp/devcage-data`
3. `./.devcage-data`

Prints a stderr note when relocating. Explicit `DEVCAGE_DATA_DIR` is never overridden.

Optional pin for predictability:

```bash
export DEVCAGE_DATA_DIR="${TMPDIR:-/tmp}/devcage-data"
```

### Practical recipe

```bash
cd /path/to/project
export PATH="/path/to/devcage-install:$PATH"
grok --sandbox keys-first
# inside agent work: devcage run -p offline -- …
```

## What DevCage is *not* in this model

- Not a replacement for mcp-guard / secret-launcher / pkg-guard
- Not “put Grok inside Alpine so skills still work” (they won’t, cleanly)
- Not an MCP that reimplements every built-in tool

DevCage is the **optional deep cage** for execution venue when sandbox + permissions are not enough.

## Related

- [architecture.md](./architecture.md) — backends and closed decisions
- [usage.md](./usage.md) — CLI
- [agent-images.md](./agent-images.md) — container images when you *do* cage commands
- Grok user guide: sandbox + permissions (`~/.grok/docs/user-guide/18-sandbox.md`, `22-permissions-and-safety.md`)
