# MCP connector isolation

`devcage mcp` is an opt-in link in the same CLI chain as mcp-guard and mcp-secret-launcher. The agent stays on the host. Only the connector you put behind `devcage mcp` is boxed.

That process is limited to:

- the secret `mcp-secret-launcher` injected for **this** profile
- network (`allowed` by default, `--net none` to block)
- a workspace only if you pass `--share-workspace ro` or `rw`
- an ephemeral home that is wiped when the process exits

It does not get Keychain, the SSH agent, or the rest of the host home.

```json
{
  "mcpServers": {
    "boxed-server": {
      "command": "mcp-guard",
      "args": [
        "run",
        "--policy",
        "/absolute/path/policy.toml",
        "mcp-secret-launcher",
        "run",
        "--profile",
        "boxed-server",
        "--",
        "devcage",
        "mcp",
        "--",
        "uvx",
        "my-server==1.2.3"
      ]
    },
    "left-alone": {
      "command": "uvx",
      "args": ["another-server==1.0.0"]
    }
  }
}
```

`left-alone` is unchanged. `boxed-server` is the one connector in the cage.

Light wrap is the default (Seatbelt on macOS, bwrap on Linux). `--container` runs a Linux guest; on macOS a Darwin `uvx` / `npx` binary cannot run there.

```bash
devcage mcp --print -- uvx my-server==1.2.3
devcage mcp --net none --share-workspace ro -- npx -y some-server@1.0.0
```
