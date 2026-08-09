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
```

Full model: [docs/recommended-stack.md](../../docs/recommended-stack.md).
