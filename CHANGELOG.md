# Changelog

All notable changes to **DevCage** are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- **Linux `bwrap` DNS.** `devcage wrap` and `devcage mcp` bind `/etc` read-only. When `/etc/resolv.conf` is a symlink (systemd-resolved `stub-resolv.conf` on `/run`), that target was outside the cage, so name lookup failed. The plan now bind-mounts the resolved file read-only and recreates only the directories and symlinks needed to follow it.

### Added

- **`devcage mcp` connector isolation.** Opt-in CLI link, same shape as mcp-guard and mcp-secret-launcher. The agent stays on the host. The boxed server keeps the secret injected for its profile and does not get Keychain, the SSH agent, or the rest of the host home. Light wrap (Seatbelt / bwrap) by default; `--container` is optional. Missing an OS sandbox fails closed unless `--allow-env-only`.

- **One-line install:** `curl -fsSL https://raw.githubusercontent.com/suryan/devcage/main/scripts/install.sh | bash -s -- --yes` clones, builds, and installs to `$HOME/.local/bin`. From a checkout, `./scripts/install.sh` still builds that tree. Flags: `--yes`, `--ref`, `--prefix`, `--local`, `--no-keep-src`. Rustup is bootstrapped when `cargo` is missing.

## [0.7.0] - 2026-08-13

### Added

- **`devcage wrap` on Linux:** `bwrap` argv plan (`src/bwrap_plan.rs`) unit-tested on macOS; used at runtime when `bwrap` is on `PATH` (`--unshare-net` for `--net none`).

### Fixed

- Linux release build: gate unused `seatbelt_enabled` behind `cfg(target_os = "macos")` so `dead_code` does not fail the compile.
- **`devcage grok` on Linux:** generated `keys-first` no longer sets `restrict_network=true`. Grok's seccomp filter also blocked `connect()` to `/var/run/docker.sock`, so nested `devcage run -p offline` failed inside the combined session. Hard no-network remains the offline/untrusted container.
- **`keys-first` `.env` deny on Linux:** seed exact `.env` / `local.env` / `.env.local` (plus existing globs) so bind-over covers files created after start. Generated recipes auto-refresh on the next `devcage grok` / `init-grok`.
- `devcage limits` no longer overwrites or deletes a real project `.env` when probing secret-write deny.

## [0.6.0] - 2026-08-11

### Added

- **`auth_seed`**: copy `~/.grok/auth.json` (+ config) into ephemeral HOME for `agent-host` so Grok can authenticate without re-login.
- **Recommended stack** doc: Grok built-ins + sandbox/permissions + mcp-guard / secret-launcher / pkg-guard; DevCage as optional deep cage ([docs/recommended-stack.md](docs/recommended-stack.md)).
- Example Grok recipes: [examples/grok/](examples/grok/) (`sandbox.toml`, `config.toml`).
- **Grok sandbox composition:** auto-fallback `data_dir` when the default (`~/.local/share/devcage`) is not writable (`$TMPDIR` → `/tmp` → `./.devcage-data`); clearer `ensure` errors; `doctor.data_dir_note`.
- **`devcage grok`** combined mode: seed `.grok` recipes, set `DEVCAGE_DATA_DIR` + `PATH`, launch `grok --sandbox keys-first` (override with `--sandbox` / `DEVCAGE_GROK_SANDBOX`).
- **`devcage init-grok`**: write embedded sandbox/permission recipes into the project.
- **`docs/usage.md`**: full Grok operator guide with step-by-step meaning (setup, daily, deep cage, recipes, limits, troubleshooting).
- **`devcage limits`**: JSON probes for sandbox + nested cage composition.
- **Hardening:** `keys-first` extends Grok **`strict`** + expanded deny; `devcage grok` scrubs `SSH_AUTH_SOCK`/cloud tokens; stable PATH; `DEVCAGE_DENY_HOST=1` blocks profile `host` (override `--allow-host-profile`).
- **`devcage wrap`**: run **any** agent/command under DevCage process isolation (macOS Seatbelt + env scrub + nested deep-cage wiring); no agent-native sandbox required.
- **`wrap --net none`**: Seatbelt denies process network.
- **Tighter wrap FS**: no wholesale `$HOME` read; named agent/tool directories only.
- **`devcage limits`**: wrap `--print` / `--net none` probes.

### Changed

- Default agent is **`grok`** (`DEVCAGE_DEFAULT_AGENT`); Claude is not required.
- Product positioning: daily path is **process containment of Grok**, not “agent always inside container.”
- `agent-host` keeps `XAI_API_KEY` while still scrubbing cloud/SSH ambient secrets.
- Clippy: **`all` + `pedantic` fully denied** (no pedantic opt-outs). Refactors: `lib`+thin `main`, policy enums, `PlanRequest`, split verify helpers.
- Prefer code fixes over `#[allow(clippy::…)]`.

## [0.5.1] - 2026-08-09

### Notes

Colima e2e verified; setup-runtime helper

### Changed

- Version bump to 0.5.1 (see releases/v0.5.1.md).


### Fixed

- Quiet `docker kill` when container already removed (`--rm`).
- Smoke tests adapt when Docker/Colima is present.

### Added

- Container e2e tests (`tests/container_e2e.rs`) — verified offline network block + agent net.

## [0.5.0] - 2026-08-09

### Notes

seatbelt opt-in, events/logs, net-allowlist advisory

### Changed

- Version bump to 0.5.0 (see releases/v0.5.0.md).

### Added

- Experimental macOS **Seatbelt** path for `agent-host` (`DEVCAGE_SEATBELT=1`).
- Session **event logs** (`events.jsonl`) + `devcage logs <session>`.
- **`net-allowlist` profile** (advisory): loads host list, sets `DEVCAGE_NET_ALLOWLIST` env.
- `devcage init-allowlist` + `examples/net-allowlist.txt`.

## [0.4.0] - 2026-08-09

### Notes

agent-host fallback, export, agent-images docs

### Changed

- Version bump to 0.4.0 (see releases/v0.4.0.md).


### Added

- Profile **`agent-host`**: host-restricted agent (ephemeral HOME + env scrub).
- `devcage agent --host-restricted` and default host fallback when no container (`DEVCAGE_AGENT_HOST_FALLBACK`).
- `devcage export <session> <src> <dest>` for promoting artifacts out of session home.
- Docs: [docs/agent-images.md](docs/agent-images.md).
- `--keep` on `run`/`shell` to override profile default wipe.
- Doctor `readiness` field (`ready` | `degraded_host_only` | `not_ready`).
- Host profile strips common secret env vars from the child process.

## [0.3.0] - 2026-08-09

### Notes

untrusted profile, verify/pull/clean, container hardening

### Changed

- Version bump to 0.3.0 (see releases/v0.3.0.md).


### Added

- Profile **`untrusted`**: network=none, hardened container (`--cap-drop ALL`, `no-new-privileges`), always wipe, no host package caches.
- Commands: **`verify`** (isolation smoke checks), **`pull`** (image), **`clean`** (wipe session data).
- Offline/untrusted default wipe; container hardening flags for those profiles.
- Refuse sensitive workspaces (`.ssh`, `.aws`, …).

## [0.2.0] - 2026-08-09

### Notes

MVP: container isolation for agent/offline, host break-glass, quality gates

### Changed

- Version bump to 0.2.0 (see releases/v0.2.0.md).


### Added

- **MVP runtime:** container backend (Docker/Podman) for `agent` / `offline`; break-glass `host` backend.
- `run`/`shell` `--wipe`; workspace defaults to git root when present.
- Working CLI: `doctor`, `status`, `profile`, `run`, `shell`, `agent`, `stop [--wipe]`.
- Session store under data dir; start banner with honest network labels; fail closed without runtime.
- Optional RO host package cache mounts (`DEVCAGE_SHARE_PACKAGE_CACHES`).
- Project quality gate: strict Clippy (pedantic/nursery + selected restriction), `rustfmt`, ShellCheck.
- Coverage gate via `cargo llvm-cov` (`scripts/coverage.sh`, default ≥ 80% lines).
- Pre-commit hooks (`./scripts/setup-hooks.sh`, `.pre-commit-config.yaml`).
- `CHANGELOG.md`, `releases/` notes, and `scripts/bump-version.sh`.
- Project rules in `AGENTS.md` (always run checks; always commit after changes).

## [0.1.0] - 2026-08-09

### Added

- Scaffold Rust CLI (`doctor`, `status`, `profile`, `agent`, `run`, `shell`, `stop`).
- Product docs: PRD, architecture, usage; agent-first wedge (`agent` / `offline` / `host`).
- Scripts: `build.sh`, `check.sh`, `install.sh`.
