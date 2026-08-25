#!/usr/bin/env python3
"""GitNexus MCP helper — auto-start `npx gitnexus serve` + call MCP tools.
(GitNexus MCP helper — tự chạy server + gọi MCP tool qua HTTP)

Usage:
  gitnexus-mcp.py tools/list
  gitnexus-mcp.py tools/call <name> [<args-json>]
  gitnexus-mcp.py resources/read <uri>
"""
import json
import os
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request

PORT = os.environ.get("GITNEXUS_PORT", "4747")
BASE = f"http://127.0.0.1:{PORT}/api/mcp"


def _alive() -> bool:
    try:
        urllib.request.urlopen(BASE, timeout=2)
        return True
    except urllib.error.HTTPError:
        return True  # server phản hồi (400/405/...) = đang chạy
    except Exception:
        return False


def rpc(sid, payload):
    headers = {
        "Content-Type": "application/json",
        "Accept": "application/json, text/event-stream",
    }
    if sid:
        headers["Mcp-Session-Id"] = sid
    req = urllib.request.Request(BASE, data=json.dumps(payload).encode(), headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            sid = sid or resp.headers.get("mcp-session-id", "")
            # SSE: read until empty line after an event block — đọc tới hết event
            chunks = []
            while True:
                line = resp.readline()
                if not line:
                    break
                line = line.decode()
                if line == "\n" and chunks and chunks[-1].startswith("data:"):
                    break
                if line == "\n" and chunks:
                    chunks.append(line)
                    continue
                chunks.append(line)
            return sid, "".join(chunks)
    except Exception as e:
        return sid, f"ERROR: {e}"


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(2)

    # Auto-start server if not running — tự khởi động server nếu chưa chạy
    if not _alive():
        print(f"[gitnexus] starting: npx gitnexus@latest serve --port {PORT}", file=sys.stderr)
        log = open("/tmp/gitnexus-serve.log", "w")
        subprocess.Popen(
            ["npx", "gitnexus@latest", "serve", "--port", PORT],
            stdout=log, stderr=subprocess.STDOUT, start_new_session=True,
        )
        for _ in range(90):
            time.sleep(1)
            if _alive():
                break
        if not _alive():
            print("[gitnexus] FAILED to start — xem /tmp/gitnexus-serve.log", file=sys.stderr)
            sys.exit(1)
        print(f"[gitnexus] running on http://localhost:{PORT}", file=sys.stderr)

    sid, _ = rpc(None, {
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2025-03-26", "capabilities": {},
                   "clientInfo": {"name": "mgc-agent", "version": "1"}},
    })

    cmd = sys.argv[1]
    if cmd == "tools/list":
        sid, out = rpc(sid, {"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
    elif cmd == "resources/read":
        sid, out = rpc(sid, {"jsonrpc": "2.0", "id": 2, "method": "resources/read",
                             "params": {"uri": sys.argv[2]}})
    elif cmd == "tools/call":
        name = sys.argv[2]
        args = json.loads(sys.argv[3]) if len(sys.argv) > 3 else {}
        sid, out = rpc(sid, {"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                             "params": {"name": name, "arguments": args}})
    else:
        print(f"Unknown command: {cmd}", file=sys.stderr)
        sys.exit(2)

    # Parse SSE frames — parse khung SSE
    if "data:" not in out:
        print(out)
        return
    for m in re.findall(r"data: (.*)", out):
        d = json.loads(m)
        if "result" in d:
            r = d["result"]
            if "content" in r:
                for x in r["content"]:
                    if x.get("type") == "text":
                        print(x["text"])
            elif "tools" in r:
                print("\n".join(t["name"] for t in r["tools"]))
            elif "contents" in r:
                print("\n\n".join(c.get("text", "") for c in r["contents"]))
            else:
                print(json.dumps(r, indent=2)[:4000])
        elif "error" in d:
            print(json.dumps(d["error"], indent=2))


if __name__ == "__main__":
    main()
