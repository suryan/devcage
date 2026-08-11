# Changelog

All notable changes to **DevCage** are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **`auth_seed`**: copy `~/.grok/auth.json` (+ config) into ephemeral HOME for `agent-host` so Grok can authenticate without re-login.
- **Recommended stack** doc: Grok built-ins + sandbox/permissions + mcp-guard / secret-launcher / pkg-guard; DevCage as optional deep cage ([docs/recommended-stack.md](docs/recommended-stack.md)).
- Example Grok recipes: [examples/grok/](examples/grok/) (`sandbox.toml`, `config.toml`).
- **Grok sandbox composition:** auto-fallback `data_dir` when the default (`~/.local/share/devcage`) is not writable (`$TMPDIR` → `/tmp` → `./.devcage-data`); clearer `ensure` errors; `doctor.data_dir_note`.
- **`devcage grok`** combined mode: seed `.grok` recipes, set `DEVCAGE_DATA_DIR` + `PATH`, launch `grok --sandbox keys-first` (override with `--sandbox` / `DEVCAGE_GROK_SANDBOX`).
- **`devcage init-grok`**: write embedded sandbox/permission recipes into the project.
- **`docs/usage.md`**: full Grok operator guide (`devcage grok`, nested deep cage, data-dir fallback, agent vs grok, troubleshooting).
- **`devcage limits`**: JSON probes for sandbox + nested cage composition.
- **Hardening:** `keys-first` extends Grok **`strict`** + expanded deny; `devcage grok` scrubs `SSH_AUTH_SOCK`/cloud tokens; stable PATH; `DEVCAGE_DENY_HOST=1` blocks profile `host` (override `--allow-host-profile`).

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
