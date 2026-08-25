# mgc-mcp — MCP server cho agent (P2)

MCP server (stdio, JSON-RPC 2.0) — không dependency, giảm 1 hop:

```shell
python3 mcp_server.py
```

Tools (gọi thẳng `mgc` binary — đã có `--json` mọi lệnh):

| Tool | Lệnh |
|---|---|
| `install` | `mgc install <pkgs>` |
| `add` | `mgc add <pkgs>` |
| `publish` | `mgc publish --json [--dry-run]` |
| `dedupe` | `mgc dedupe --json` |
| `audit` | `mgc audit` |

Cấu hình agent (vd Claude Code):

```json
{
  "mcpServers": {
    "mgc-mcp": {
      "command": "python3",
      "args": ["<abs>/tools/mgc-mcp/mcp_server.py"]
    }
  }
}
```

Chính sách: tool CHỈ chạy mgc — security check của CLI (trust, quarantine, audit-strict) giữ nguyên, không bypass.