#!/usr/bin/env python3
"""mg-mcp — MCP server cho agent (P2, 18 §18).

Stdio JSON-RPC 2.0, không dependency ngoài stdlib.
Tools gọi `mg <cmd> --json` (CLI đã xuất JSON chuẩn) — 1 hop, không wrap chồng.

Run: python3 mcp_server.py   (stdio config trong agent: mcpServers.mg-mcp.command)
"""
import json
import shutil
import subprocess
import sys

MG = shutil.which("mg") or "mg"

TOOLS = [
    {
        "name": "install",
        "description": "Install packages (mg install)",
        "inputSchema": {"type": "object", "properties": {"packages": {"type": "array", "items": {"type": "string"}, "description": "package specs (name@version)"}}, "required": ["packages"]},
    },
    {
        "name": "add",
        "description": "Add dependencies to project (mg add)",
        "inputSchema": {"type": "object", "properties": {"packages": {"type": "array", "items": {"type": "string"}}, "required": ["packages"]}},
    },
    {
        "name": "publish",
        "description": "Publish package (mg publish --json)",
        "inputSchema": {"type": "object", "properties": {"dry_run": {"type": "boolean", "description": "xem trước, không publish"}}},
    },
    {
        "name": "dedupe",
        "description": "Dedupe packages (mg dedupe)",
        "inputSchema": {"type": "object", "properties": {}},
    },
    {
        "name": "audit",
        "description": "Security audit (mg audit --json)",
        "inputSchema": {"type": "object", "properties": {}},
    },
]


def run_mg(args, cwd=None):
    try:
        proc = subprocess.run([MG, *args], capture_output=True, text=True, cwd=cwd, timeout=300)
        out = proc.stdout.strip()
        try:
            parsed = json.loads(out) if out else {}
        except json.JSONDecodeError:
            parsed = {"raw": out}
        return {"ok": proc.returncode == 0, "exit": proc.returncode, "result": parsed}
    except subprocess.TimeoutExpired:
        return {"ok": False, "exit": -1, "result": {"error": "timeout"}}
    except FileNotFoundError:
        return {"ok": False, "exit": -1, "result": {"error": f"mg binary not found ({MG})"}}
    except Exception as e:  # noqa: BLE001
        return {"ok": False, "exit": -1, "result": {"error": str(e)}}


ARGS_FOR_TOOL = {
    "install": lambda p: ["install", *p.get("packages", [])],
    "add": lambda p: ["add", *p.get("packages", [])],
    "publish": lambda p: ["publish", "--json", "--dry-run"] if p.get("dry_run") else ["publish", "--json"],
    "dedupe": lambda p: ["dedupe", "--json"],
    "audit": lambda p: ["audit"],
}


def handle_call(params):
    name = params.get("name", "")
    args = params.get("arguments", {}) or {}
    build = ARGS_FOR_TOOL.get(name)
    if build is None:
        return {"isError": True, "content": [{"type": "text", "text": f"unknown tool: {name}"}]}
    res = run_mg(build(args))
    return {"content": [{"type": "text", "text": json.dumps(res, ensure_ascii=False)}]}


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        rid = msg.get("id")
        method = msg.get("method")
        params = msg.get("params") or {}
        result = None
        if method == "initialize":
            result = {
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {"listChanged": False}},
                "serverInfo": {"name": "mg-mcp", "version": "0.2.0"},
            }
        elif method == "tools/list":
            result = {"tools": TOOLS}
        elif method == "tools/call":
            result = handle_call(params)
        else:
            result = None
        if rid is not None:
            reply = {"jsonrpc": "2.0", "id": rid}
            if result is not None:
                reply["result"] = result
            else:
                reply["error"] = {"code": -32601, "message": f"method not found: {method}"}
            sys.stdout.write(json.dumps(reply) + "\n")
            sys.stdout.flush()


if __name__ == "__main__":
    main()