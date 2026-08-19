# mg-mcp — MCP server cho agent (P2)

MCP server (stdio, JSON-RPC 2.0) — không dependency, giảm 1 hop:

```shell
python3 mcp_server.py
```

Tools (gọi thẳng `mg` binary — đã có `--json` mọi lệnh):

| Tool | Lệnh |
|---|---|
| `install` | `mg install <pkgs>` |
| `add` | `mg add <pkgs>` |
| `publish` | `mg publish --json [--dry-run]` |
| `dedupe` | `mg dedupe --json` |
| `audit` | `mg audit` |

Cấu hình agent (vd Claude Code):

```json
{
  "mcpServers": {
    "mg-mcp": {
      "command": "python3",
      "args": ["<abs>/tools/mg-mcp/mcp_server.py"]
    }
  }
}
```

Chính sách: tool CHỈ chạy mg — security check của CLI (trust, quarantine, audit-strict) giữ nguyên, không bypass.