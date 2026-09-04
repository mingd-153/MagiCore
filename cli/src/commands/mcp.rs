//! `commands/mcp.rs` — Native Built-in Model Context Protocol (MCP) Server.
//!
//! Provides direct JSON-RPC 2.0 stdio stream communication for AI Coding Assistants
//! (Cursor, Windsurf, Claude Code, Devin, Antigravity) without external Python runtimes.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[serde(rename = "jsonrpc")]
    _jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

pub async fn run() -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut line = String::new();

    while reader.read_line(&mut line)? > 0 {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }

        if let Ok(req) = serde_json::from_str::<JsonRpcRequest>(trimmed) {
            let response = handle_rpc_request(&req).await;
            let res_bytes = serde_json::to_vec(&response)?;
            stdout.write_all(&res_bytes)?;
            stdout.write_all(b"\n")?;
            stdout.flush()?;
        }

        line.clear();
    }

    Ok(())
}

async fn handle_rpc_request(req: &JsonRpcRequest) -> JsonRpcResponse {
    match req.method.as_str() {
        "initialize" => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: req.id.clone(),
            result: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "magicore-native-mcp",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
            error: None,
        },
        "tools/list" => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: req.id.clone(),
            result: Some(json!({
                "tools": [
                    {
                        "name": "mgc_install",
                        "description": "Execute MagiCore package installation with CAS reflink/hardlink caching",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "packages": {
                                    "type": "array",
                                    "items": { "type": "string" },
                                    "description": "Optional list of package specs (name@version) to install"
                                },
                                "frozen": {
                                    "type": "boolean",
                                    "description": "Fail if lockfile needs update (CI mode)"
                                }
                            }
                        }
                    },
                    {
                        "name": "mgc_add",
                        "description": "Add dependencies to the current polyglot project",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "packages": {
                                    "type": "array",
                                    "items": { "type": "string" },
                                    "description": "List of packages to add"
                                },
                                "dev": {
                                    "type": "boolean",
                                    "description": "Add as devDependency"
                                }
                            },
                            "required": ["packages"]
                        }
                    },
                    {
                        "name": "mgc_audit",
                        "description": "Run supply-chain security audit and 24-hour release quarantine checks",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "fix": {
                                    "type": "boolean",
                                    "description": "Automatically bump vulnerable packages if safe"
                                }
                            }
                        }
                    },
                    {
                        "name": "mgc_workspace_info",
                        "description": "Get monorepo topology, computation build cache and catalog mappings",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    }
                ]
            })),
            error: None,
        },
        "tools/call" => {
            let tool_name = req
                .params
                .as_ref()
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or_default();

            // P0.4 FIX: Call REAL commands instead of hardcoded stubs
            let tool_res = match tool_name {
                "mgc_install" => {
                    // Extract params
                    let packages = req
                        .params
                        .as_ref()
                        .and_then(|p| p.get("arguments"))
                        .and_then(|a| a.get("packages"))
                        .and_then(|p| p.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();

                    let frozen = req
                        .params
                        .as_ref()
                        .and_then(|p| p.get("arguments"))
                        .and_then(|a| a.get("frozen"))
                        .and_then(|f| f.as_bool())
                        .unwrap_or(false);

                    // Call REAL install command
                    match crate::commands::install::run(
                        packages.clone(),
                        None,  // core: detect from project
                        false, // ignore_scripts
                        true,  // allow_scripts (default)
                        false, // offline
                        frozen, // frozen mode (CI): fail if lockfile needs update
                    )
                    .await
                    {
                        Ok(()) => {
                            let pkg_list = if packages.is_empty() {
                                "all from manifest".to_string()
                            } else {
                                packages.join(", ")
                            };
                            json!({
                                "content": [{
                                    "type": "text",
                                    "text": format!(
                                        "✅ MagiCore install completed{}\nPackages: {}",
                                        if frozen { " (frozen mode)" } else { "" },
                                        pkg_list
                                    )
                                }]
                            })
                        }
                        Err(e) => json!({
                            "content": [{
                                "type": "text",
                                "text": format!("❌ Install failed: {}", e)
                            }],
                            "isError": true
                        }),
                    }
                }
                "mgc_audit" => {
                    let fix = req
                        .params
                        .as_ref()
                        .and_then(|p| p.get("arguments"))
                        .and_then(|a| a.get("fix"))
                        .and_then(|f| f.as_bool())
                        .unwrap_or(false);

                    // Call REAL audit command
                    match crate::commands::audit::run(None, fix).await {
                        Ok(()) => json!({
                            "content": [{
                                "type": "text",
                                "text": format!(
                                    "✅ Security audit completed{}",
                                    if fix { " (auto-fix applied)" } else { "" }
                                )
                            }]
                        }),
                        Err(e) => json!({
                            "content": [{
                                "type": "text",
                                "text": format!("❌ Audit failed: {}", e)
                            }],
                            "isError": true
                        }),
                    }
                }
                "mgc_workspace_info" => {
                    use crate::commands::workspace::WorkspaceCmd;
                    // Call REAL workspace command (list)
                    match crate::commands::workspace::run(WorkspaceCmd::List {
                        filter: None,
                        json: true, // Return JSON for MCP
                    })
                    .await
                    {
                        Ok(()) => json!({
                            "content": [{
                                "type": "text",
                                "text": "✅ Workspace info retrieved (see output above)"
                            }]
                        }),
                        Err(e) => json!({
                            "content": [{
                                "type": "text",
                                "text": format!("❌ Workspace query failed: {}", e)
                            }],
                            "isError": true
                        }),
                    }
                }
                "mgc_add" => {
                    // Extract params
                    let packages = req
                        .params
                        .as_ref()
                        .and_then(|p| p.get("arguments"))
                        .and_then(|a| a.get("packages"))
                        .and_then(|p| p.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();

                    let dev = req
                        .params
                        .as_ref()
                        .and_then(|p| p.get("arguments"))
                        .and_then(|a| a.get("dev"))
                        .and_then(|d| d.as_bool())
                        .unwrap_or(false);

                    if packages.is_empty() {
                        json!({
                            "content": [{
                                "type": "text",
                                "text": "❌ mgc_add requires at least one package"
                            }],
                            "isError": true
                        })
                    } else {
                        // Call REAL add command
                        match crate::commands::add::run_many(
                            packages.clone(),
                            None, // version
                            dev,
                            false, // exact
                            false, // optional
                            false, // peer
                            false, // no_save
                            false, // global
                            None,  // core: detect from project
                        )
                        .await
                        {
                            Ok(()) => json!({
                                "content": [{
                                    "type": "text",
                                    "text": format!(
                                        "✅ Added {} package(s){}",
                                        packages.len(),
                                        if dev { " (dev)" } else { "" }
                                    )
                                }]
                            }),
                            Err(e) => json!({
                                "content": [{
                                    "type": "text",
                                    "text": format!("❌ Add failed: {}", e)
                                }],
                                "isError": true
                            }),
                        }
                    }
                }
                _ => json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Unknown tool: {tool_name}")
                    }],
                    "isError": true
                }),
            };

            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: req.id.clone(),
                result: Some(tool_res),
                error: None,
            }
        }
        _ => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: req.id.clone(),
            result: None,
            error: Some(json!({
                "code": -32601,
                "message": format!("Method not found: {}", req.method)
            })),
        },
    }
}
