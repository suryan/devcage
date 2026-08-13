# Grok recipes (keys-first + DevCage optional)

Copy into a **consumer project** (not required inside the DevCage repo itself):

```bash
mkdir -p /path/to/your-app/.grok
cp sandbox.toml /path/to/your-app/.grok/
cp config.toml  /path/to/your-app/.grok/
```

```bash
cd /path/to/your-app
# Preferred:
devcage grok
# After DevCage upgrades, re-seed recipes:
devcage init-grok --force
devcage limits
```

Recipes: `keys-first` extends Grok **`strict`**, exact `.env` deny + globs, child network left on (Linux Docker), permission deny for host break-glass.

Full model: [docs/usage.md](../../docs/usage.md) · [recommended-stack.md](../../docs/recommended-stack.md).
