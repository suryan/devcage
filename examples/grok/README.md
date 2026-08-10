# Grok recipes (keys-first + DevCage optional)

Copy into a **consumer project** (not required inside the DevCage repo itself):

```bash
mkdir -p /path/to/your-app/.grok
cp sandbox.toml /path/to/your-app/.grok/
cp config.toml  /path/to/your-app/.grok/
```

```bash
cd /path/to/your-app
# Required if this Grok session will call `devcage run` (sandbox blocks ~/.local/share):
export DEVCAGE_DATA_DIR="${TMPDIR:-/tmp}/devcage-data"
grok --sandbox keys-first
```

**Composition note:** Grok sandbox + DevCage was smoke-tested (macOS + Colima): default DevCage data dir fails under sandbox (`Operation not permitted`); with `DEVCAGE_DATA_DIR` under `/tmp`, `devcage run -p offline` and `host` succeed. See [docs/recommended-stack.md](../../docs/recommended-stack.md#do-they-work-together-tested).

Full model: [docs/recommended-stack.md](../../docs/recommended-stack.md).
