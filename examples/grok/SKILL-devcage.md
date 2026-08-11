# Project skill: DevCage deep cage (optional)

Copy into a consumer project's skills/rules if useful.

## Rule

For untrusted scripts, offline builds, or Linux-only commands in this repo:

1. Prefer `devcage run -p offline -- …` or `devcage run -p untrusted -- …`.
2. Do **not** use `devcage run -p host` (blocked under `devcage grok` unless explicitly allowed).
3. Do not read `~/.ssh`, `~/.aws`, `.env`, or Keychain material; use MCP via mcp-guard + secret-launcher for keys.
4. Host network may still work under Grok sandbox on macOS — use offline profile when network must be off.

Daily entrypoint: `devcage grok` (not bare `grok`).
