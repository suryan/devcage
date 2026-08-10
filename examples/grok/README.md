# Grok recipes (keys-first + DevCage optional)

Copy into a **consumer project** (not required inside the DevCage repo itself):

```bash
mkdir -p /path/to/your-app/.grok
cp sandbox.toml /path/to/your-app/.grok/
cp config.toml  /path/to/your-app/.grok/
```

```bash
cd /path/to/your-app
grok --sandbox keys-first
# DevCage auto-relocates session data if ~/.local/share is blocked.
# Optional pin: export DEVCAGE_DATA_DIR="${TMPDIR:-/tmp}/devcage-data"
```

**Composition:** DevCage write-probes the default data dir and falls back to `$TMPDIR` under Grok sandbox. See [docs/recommended-stack.md](../../docs/recommended-stack.md#do-they-work-together-tested).

Full model: [docs/recommended-stack.md](../../docs/recommended-stack.md).
