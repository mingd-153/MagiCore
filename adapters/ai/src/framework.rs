//! AI framework detection for mgc-ai-adapter.
//! Nhận diện framework AI qua mgc.toml hoặc pyproject tool.magicore.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiFramework {
    PythonAgent,
    McpServer,
}

impl AiFramework {
    pub fn as_str(&self) -> &'static str {
        match self {
            AiFramework::PythonAgent => "python-agent",
            AiFramework::McpServer => "mcp-server",
        }
    }

    pub fn entry_script(&self) -> &'static str {
        match self {
            AiFramework::PythonAgent => "src/agent.py",
            AiFramework::McpServer => "server.py",
        }
    }
}

pub fn detect_framework(root: &Path) -> Option<AiFramework> {
    // Try explicit framework in mgc.toml — ưu tiên khai báo MagiCore rõ ràng.
    if let Ok(content) = std::fs::read_to_string(root.join("mgc.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(p) = v
                .get("ai")
                .and_then(|c| c.get("framework"))
                .and_then(|p| p.as_str())
            {
                return framework_from_str(p);
            }
        }
    }

    // Try explicit framework in pyproject.toml — hỗ trợ project Python đã khai báo core.
    if let Ok(content) = std::fs::read_to_string(root.join("pyproject.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(p) = v
                .get("tool")
                .and_then(|t| t.get("magicore"))
                .and_then(|m| m.get("framework"))
                .and_then(|p| p.as_str())
            {
                return framework_from_str(p);
            }
        }
    }

    // Generic Python fallback — nhận project Python thường để install-ai/dev-ai chạy được.
    // Default Python agent — mặc định an toàn cho dependency management cơ bản.
    if root.join("pyproject.toml").exists() || root.join("requirements.txt").exists() {
        return Some(AiFramework::PythonAgent);
    }

    None
}

fn framework_from_str(s: &str) -> Option<AiFramework> {
    match s {
        "python-agent" => Some(AiFramework::PythonAgent),
        "mcp-server" => Some(AiFramework::McpServer),
        _ => None,
    }
}
