//! `commands/mcp.rs` — Native Built-in Model Context Protocol (MCP) Server.
//!
//! Provides direct JSON-RPC 2.0 stdio stream communication for AI Coding Assistants
//! (Cursor, Windsurf, Claude Code, Devin, Antigravity) without external Python runtimes.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs::File;
use std::io::{self, BufRead, Read, Seek, SeekFrom, Write};

/// Bound command output returned through MCP while keeping disk-backed capture.
/// Giới hạn output command trả qua MCP, dùng file tạm để không phình bộ nhớ.
const MAX_MCP_CAPTURE_BYTES: u64 = 1_048_576;

/// Keep the JSON-RPC pipe separate while CLI command output goes to stderr.
/// Tách pipe JSON-RPC, chuyển output CLI sang stderr.
struct McpProtocolOutput {
    protocol: File,
    #[cfg(windows)]
    original_stdout: WinHandle,
}

#[cfg(windows)]
type WinHandle = *mut std::ffi::c_void;

#[cfg(windows)]
const WIN_STD_OUTPUT_HANDLE: u32 = (-11_i32) as u32;
#[cfg(windows)]
const WIN_STD_ERROR_HANDLE: u32 = (-12_i32) as u32;

#[cfg(windows)]
#[link(name = "kernel32")]
#[allow(unsafe_code)]
unsafe extern "system" {
    fn GetCurrentProcess() -> WinHandle;
    fn GetStdHandle(std_handle: u32) -> WinHandle;
    fn SetStdHandle(std_handle: u32, handle: WinHandle) -> i32;
    fn DuplicateHandle(
        source_process: WinHandle,
        source_handle: WinHandle,
        target_process: WinHandle,
        target_handle: *mut WinHandle,
        desired_access: u32,
        inherit_handle: i32,
        options: u32,
    ) -> i32;
    fn CloseHandle(handle: WinHandle) -> i32;
}

impl McpProtocolOutput {
    // SAFETY: only duplicates/redirects the process stdio handles; the saved handle is owned by `File`.
    // AN TOÀN: chỉ nhân đôi/chuyển hướng handle stdio; handle đã lưu được `File` sở hữu.
    #[allow(unsafe_code)]
    fn redirect_cli_output() -> Result<Self> {
        #[cfg(unix)]
        {
            use std::os::fd::FromRawFd;

            // Duplicate protocol stdout, then send ordinary fd 1 output to stderr.
            // Nhân đôi stdout giao thức, rồi chuyển output fd 1 thông thường sang stderr.
            let protocol_fd = unsafe { libc::dup(libc::STDOUT_FILENO) };
            if protocol_fd < 0 {
                return Err(io::Error::last_os_error()).context("cannot preserve MCP stdout");
            }
            if unsafe { libc::dup2(libc::STDERR_FILENO, libc::STDOUT_FILENO) } < 0 {
                let error = io::Error::last_os_error();
                unsafe { libc::close(protocol_fd) };
                return Err(error).context("cannot redirect CLI output to stderr");
            }
            let protocol = unsafe { File::from_raw_fd(protocol_fd) };
            Ok(Self { protocol })
        }

        #[cfg(windows)]
        {
            use std::os::windows::io::FromRawHandle;

            let original_stdout = unsafe { GetStdHandle(WIN_STD_OUTPUT_HANDLE) };
            let stderr = unsafe { GetStdHandle(WIN_STD_ERROR_HANDLE) };
            if original_stdout.is_null() || stderr.is_null() {
                return Err(io::Error::last_os_error())
                    .context("MCP stdio handles are unavailable");
            }
            let current_process = unsafe { GetCurrentProcess() };
            let mut protocol_handle = std::ptr::null_mut();
            if unsafe {
                DuplicateHandle(
                    current_process,
                    original_stdout,
                    current_process,
                    &mut protocol_handle,
                    0,
                    0,
                    0x0000_0002,
                )
            } == 0
            {
                return Err(io::Error::last_os_error()).context("cannot preserve MCP stdout");
            }
            if unsafe { SetStdHandle(WIN_STD_OUTPUT_HANDLE, stderr) } == 0 {
                let error = io::Error::last_os_error();
                unsafe { CloseHandle(protocol_handle) };
                return Err(error).context("cannot redirect CLI output to stderr");
            }
            let protocol = unsafe { File::from_raw_handle(protocol_handle) };
            Ok(Self {
                protocol,
                original_stdout,
            })
        }

        #[cfg(not(any(unix, windows)))]
        {
            anyhow::bail!("MCP stdio isolation is unsupported on this platform")
        }
    }
}

impl Write for McpProtocolOutput {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.protocol.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.protocol.flush()
    }
}

impl Drop for McpProtocolOutput {
    // SAFETY: restores stdout from the live protocol handle before its `File` field is dropped.
    // AN TOÀN: khôi phục stdout từ handle protocol còn sống trước khi field `File` bị drop.
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            // Restore process stdout after the MCP session ends.
            // Khôi phục stdout tiến trình khi phiên MCP kết thúc.
            unsafe { libc::dup2(self.protocol.as_raw_fd(), libc::STDOUT_FILENO) };
        }
        #[cfg(windows)]
        unsafe {
            SetStdHandle(WIN_STD_OUTPUT_HANDLE, self.original_stdout);
        }
    }
}

#[cfg(unix)]
struct McpStdoutCapture {
    output: File,
    previous_stdout: File,
    restored: bool,
}

#[cfg(not(unix))]
struct McpStdoutCapture;

impl McpStdoutCapture {
    #[allow(unsafe_code)]
    fn begin() -> Result<Self> {
        #[cfg(unix)]
        {
            use std::os::fd::{AsRawFd, FromRawFd};

            let output = tempfile::tempfile().context("cannot create MCP output capture")?;
            // Save session stderr fd 1, then redirect command output to a private temp file.
            // Lưu fd 1 của stderr phiên, rồi chuyển output command vào file tạm riêng.
            let previous_fd = unsafe { libc::dup(libc::STDOUT_FILENO) };
            if previous_fd < 0 {
                return Err(io::Error::last_os_error()).context("cannot preserve MCP stderr route");
            }
            if unsafe { libc::dup2(output.as_raw_fd(), libc::STDOUT_FILENO) } < 0 {
                let error = io::Error::last_os_error();
                unsafe { libc::close(previous_fd) };
                return Err(error).context("cannot capture command stdout");
            }
            Ok(Self {
                output,
                previous_stdout: unsafe { File::from_raw_fd(previous_fd) },
                restored: false,
            })
        }

        #[cfg(not(unix))]
        {
            Ok(Self)
        }
    }

    fn finish(mut self) -> Result<String> {
        #[cfg(unix)]
        {
            let flush_result = io::stdout().flush();
            self.restore_stdout();
            flush_result?;
            self.output.seek(SeekFrom::Start(0))?;
            let mut bytes = Vec::new();
            Read::by_ref(&mut self.output)
                .take(MAX_MCP_CAPTURE_BYTES + 1)
                .read_to_end(&mut bytes)?;
            let truncated = bytes.len() as u64 > MAX_MCP_CAPTURE_BYTES;
            if truncated {
                bytes.truncate(MAX_MCP_CAPTURE_BYTES as usize);
            }
            let mut text = String::from_utf8_lossy(&bytes).into_owned();
            if truncated {
                text.push_str("\n[command output truncated]");
            }
            Ok(text)
        }

        #[cfg(not(unix))]
        {
            Ok(String::new())
        }
    }

    #[cfg(unix)]
    #[allow(unsafe_code)]
    fn restore_stdout(&mut self) {
        use std::os::fd::AsRawFd;

        if !self.restored {
            // Restore the session stderr route before reading or replying.
            // Khôi phục đường stderr phiên trước khi đọc capture hoặc trả response.
            unsafe { libc::dup2(self.previous_stdout.as_raw_fd(), libc::STDOUT_FILENO) };
            self.restored = true;
        }
    }
}

#[cfg(unix)]
impl Drop for McpStdoutCapture {
    fn drop(&mut self) {
        self.restore_stdout();
    }
}

fn append_captured_stdout(response: &mut JsonRpcResponse, output: &str) {
    let Some(result) = response.result.as_mut() else {
        return;
    };
    let content_key = if result.get("content").is_some() {
        "content"
    } else {
        "contents"
    };
    let contents = result.get_mut(content_key).and_then(Value::as_array_mut);
    let Some(first_content) = contents.and_then(|items| items.first_mut()) else {
        return;
    };
    let Some(text) = first_content.get_mut("text") else {
        return;
    };
    let existing = text.as_str().unwrap_or_default();
    *text = Value::String(if existing.is_empty() {
        output.to_string()
    } else {
        format!("{existing}\n{output}")
    });
}

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
    let mut stdout = McpProtocolOutput::redirect_cli_output()?;
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut line = String::new();

    while reader.read_line(&mut line)? > 0 {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }

        if let Ok(req) = serde_json::from_str::<JsonRpcRequest>(trimmed) {
            let capture = McpStdoutCapture::begin()?;
            let mut response = handle_rpc_request(&req).await;
            let command_output = capture.finish()?;
            if !command_output.trim().is_empty() {
                append_captured_stdout(&mut response, command_output.trim_end());
            }
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
                    "tools": {},
                    "resources": { "listChanged": false }
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
                                },
                                "compat_runtime": {
                                    "type": "string",
                                    "description": "Explicit toolchain opt-in for delegated lanes (e.g. pip, cargo, go). Native lanes ignore it; delegated lanes FAIL CLOSED without it."
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
                    },
                    {
                        "name": "mgc_uninstall",
                        "description": "Remove dependencies through the detected core's ownership gate",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "packages": {
                                    "type": "array",
                                    "items": { "type": "string" },
                                    "description": "Package names to remove"
                                },
                                "compat_runtime": {
                                    "type": "string",
                                    "description": "Explicit toolchain opt-in for delegated lanes"
                                }
                            },
                            "required": ["packages"]
                        }
                    },
                    {
                        "name": "mgc_versions",
                        "description": "List published versions and dist-tags for a registry package",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "package": { "type": "string" }
                            },
                            "required": ["package"]
                        }
                    },
                    {
                        "name": "mgc_outdated",
                        "description": "List outdated dependencies declared in the current project",
                        "inputSchema": { "type": "object", "properties": {} }
                    },
                    {
                        "name": "mgc_config",
                        "description": "Read local configuration with sensitive values redacted",
                        "inputSchema": { "type": "object", "properties": {} }
                    }
                ]
            })),
            error: None,
        },
        "resources/list" => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: req.id.clone(),
            result: Some(json!({
                "resources": [
                    {
                        "uri": "mgc://workspace",
                        "name": "Workspace graph",
                        "description": "Workspace packages and dependency edges",
                        "mimeType": "application/json"
                    },
                    {
                        "uri": "mgc://capabilities",
                        "name": "Core capabilities",
                        "description": "Machine-readable MagiCore core capability manifest",
                        "mimeType": "application/json"
                    },
                    {
                        "uri": "mgc://config/local",
                        "name": "Local configuration",
                        "description": "Project configuration with sensitive values redacted",
                        "mimeType": "text/plain"
                    }
                ]
            })),
            error: None,
        },
        "resources/read" => {
            let uri = req
                .params
                .as_ref()
                .and_then(|params| params.get("uri"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            let content = match uri {
                "mgc://workspace" => crate::commands::workspace::workspace_info_json(),
                "mgc://capabilities" => crate::commands::capabilities::json_payload(None)
                    .and_then(|payload| serde_json::to_string_pretty(&payload).map_err(Into::into)),
                "mgc://config/local" => crate::commands::config::list_local_redacted(),
                _ => Err(anyhow::anyhow!("Unknown resource: {uri}")),
            };
            match content {
                Ok(text) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id.clone(),
                    result: Some(json!({
                        "contents": [{
                            "uri": uri,
                            "mimeType": if uri == "mgc://config/local" {
                                "text/plain"
                            } else {
                                "application/json"
                            },
                            "text": text
                        }]
                    })),
                    error: None,
                },
                Err(error) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id.clone(),
                    result: None,
                    error: Some(json!({ "code": -32002, "message": error.to_string() })),
                },
            }
        }
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

                    match crate::commands::install::run(
                        packages.clone(),
                        None,
                        false,
                        false,
                        false,
                        frozen,
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
                                        "MagiCore install completed{}\nPackages: {}",
                                        if frozen { " (frozen mode)" } else { "" },
                                        pkg_list
                                    )
                                }]
                            })
                        }
                        Err(e) => json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Install failed: {}", e)
                            }],
                            "isError": true
                        }),
                    }
                }
                "mgc_uninstall" => {
                    let arguments = req
                        .params
                        .as_ref()
                        .and_then(|params| params.get("arguments"));
                    let packages = arguments
                        .and_then(|args| args.get("packages"))
                        .and_then(Value::as_array)
                        .map(|values| {
                            values
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let compat_runtime = arguments
                        .and_then(|args| args.get("compat_runtime"))
                        .and_then(Value::as_str);
                    if packages.is_empty() {
                        json!({
                            "content": [{"type": "text", "text": "mgc_uninstall requires at least one package"}],
                            "isError": true
                        })
                    } else {
                        match crate::commands::remove::run_many(
                            packages.clone(),
                            None,
                            compat_runtime.map(str::to_string),
                        )
                        .await
                        {
                            Ok(()) => json!({
                                "content": [{"type": "text", "text": format!("Removed {} package(s)", packages.len())}]
                            }),
                            Err(error) => json!({
                                "content": [{"type": "text", "text": format!("Uninstall failed: {error:#}")}],
                                "isError": true
                            }),
                        }
                    }
                }
                "mgc_versions" => {
                    let package = req
                        .params
                        .as_ref()
                        .and_then(|params| params.get("arguments"))
                        .and_then(|args| args.get("package"))
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if package.trim().is_empty() {
                        json!({
                            "content": [{"type": "text", "text": "mgc_versions requires a package name"}],
                            "isError": true
                        })
                    } else {
                        match crate::commands::info::versions_json(package).await {
                            Ok(text) => json!({"content": [{"type": "text", "text": text}]}),
                            Err(error) => json!({
                                "content": [{"type": "text", "text": format!("Version query failed: {error:#}")}],
                                "isError": true
                            }),
                        }
                    }
                }
                "mgc_outdated" => match crate::commands::outdated::outdated_json(None).await {
                    Ok(text) => json!({"content": [{"type": "text", "text": text}]}),
                    Err(error) => json!({
                        "content": [{"type": "text", "text": format!("Outdated query failed: {error:#}")}],
                        "isError": true
                    }),
                },
                "mgc_config" => match crate::commands::config::list_local_redacted() {
                    Ok(text) => json!({"content": [{"type": "text", "text": text}]}),
                    Err(error) => json!({
                        "content": [{"type": "text", "text": format!("Config read failed: {error:#}")}],
                        "isError": true
                    }),
                },
                "mgc_audit" => {
                    let fix = req
                        .params
                        .as_ref()
                        .and_then(|p| p.get("arguments"))
                        .and_then(|a| a.get("fix"))
                        .and_then(|f| f.as_bool())
                        .unwrap_or(false);

                    match crate::commands::audit::run(None, fix, None).await {
                        Ok(()) => json!({
                            "content": [{
                                "type": "text",
                                "text": format!(
                                    "Security audit completed{}",
                                    if fix { " (auto-fix applied)" } else { "" }
                                )
                            }]
                        }),
                        Err(e) => json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Audit failed: {}", e)
                            }],
                            "isError": true
                        }),
                    }
                }
                "mgc_workspace_info" => match crate::commands::workspace::workspace_info_json() {
                    Ok(command_output) => json!({
                        "content": [{
                            "type": "text",
                            "text": command_output
                        }]
                    }),
                    Err(e) => json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Workspace query failed: {}", e)
                        }],
                        "isError": true
                    }),
                },
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

                    // Delegated-lane opt-in (Native mode when absent —
                    // delegated lanes fail closed naming the flag).
                    // (Opt-in lane delegated.)
                    let compat_runtime = req
                        .params
                        .as_ref()
                        .and_then(|p| p.get("arguments"))
                        .and_then(|a| a.get("compat_runtime"))
                        .and_then(|c| c.as_str())
                        .map(str::to_string);

                    if packages.is_empty() {
                        json!({
                            "content": [{
                                "type": "text",
                                "text": "mgc_add requires at least one package"
                            }],
                            "isError": true
                        })
                    } else {
                        match crate::commands::add::run_many(
                            packages.clone(),
                            None,
                            dev,
                            false,
                            false,
                            false,
                            false,
                            false,
                            None,
                            compat_runtime,
                        )
                        .await
                        {
                            Ok(()) => json!({
                                "content": [{
                                    "type": "text",
                                    "text": format!(
                                        "Added {} package(s){}",
                                        packages.len(),
                                        if dev { " (dev)" } else { "" }
                                    )
                                }]
                            }),
                            Err(e) => json!({
                                "content": [{
                                    "type": "text",
                                    "text": format!("Add failed: {}", e)
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
