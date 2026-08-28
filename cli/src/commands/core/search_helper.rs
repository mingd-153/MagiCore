//! Multi-registry search helper for CLI add command - SHARED FOR ALL CORES
//! Helper tìm kiếm đa registry cho lệnh CLI add - DÙNG CHUNG CHO MỌI CORE

use anyhow::Result;
use mgc_search::{
    prompt_selection, SearchCache, SearchOrchestrator, SearchQuery, SearchResult, ProjectContext,
    NpmSearchClient, CratesSearchClient, GoSearchClient, PyPISearchClient,
};
use mgc_types::PackageName;
use std::path::Path;
use std::sync::Arc;

/// Try multi-registry search for short package names
/// Thử tìm kiếm đa registry cho tên package ngắn
///
/// Returns Some(full_package_path) if user selected from search results
/// Trả về Some(đường_dẫn_package_đầy_đủ) nếu user chọn từ kết quả search
pub async fn try_multi_registry_search(
    package_name: &str,
    project_root: &Path,
) -> Result<Option<String>> {
    // Only search if package name is short (no version, no scope, no path)
    // Chỉ search nếu tên package ngắn (không version, scope, path)
    if !should_trigger_search(package_name) {
        return Ok(None);
    }
    
    mgc_ui::info("🔍 Searching across all registries...");
    
    // Detect project context
    // Phát hiện context dự án
    let context = detect_project_context(project_root)?;
    
    // Check cache for previous user choice
    // Kiểm tra cache cho lựa chọn trước của user
    let cache = SearchCache::new()?;
    if let Some(registry) = cache.get_user_choice(package_name)? {
        mgc_ui::info(&format!(
            "Auto-selecting {} from previous choices (used 3+ times)",
            registry
        ));
        // Return early with the preferred registry
        // Trả về sớm với registry ưa thích
        let full_path = format_package_path(package_name, registry);
        return Ok(Some(full_path));
    }
    
    // Create search clients
    // Tạo search clients
    let clients: Vec<Arc<dyn mgc_search::SearchClient>> = vec![
        Arc::new(NpmSearchClient::new()),
        Arc::new(CratesSearchClient::new()),
        Arc::new(GoSearchClient::new()),
        Arc::new(PyPISearchClient::new()),
    ];
    
    // Create orchestrator and search
    // Tạo orchestrator và tìm kiếm
    let orchestrator = SearchOrchestrator::new(clients);
    
    let query = SearchQuery {
        query: package_name.to_string(),
        context,
    };
    
    let results = match orchestrator.search_all(&query).await {
        Ok(r) if !r.is_empty() => r,
        Ok(_) => {
            mgc_ui::warning(&format!("No packages found for '{}'", package_name));
            return Ok(None);
        }
        Err(e) => {
            mgc_ui::warning(&format!("Search failed: {}", e));
            return Ok(None);
        }
    };
    
    // Prompt user to select
    // Prompt user chọn
    let selected = prompt_selection(&results)?;
    
    if let Some(result) = selected {
        // Track user choice for future auto-selection
        // Track lựa chọn user cho auto-select sau
        cache.track_choice(&result.name, result.registry)?;
        
        // Return full package path based on registry
        // Trả về đường dẫn package đầy đủ theo registry
        let full_path = format_package_path(&result.full_path, result.registry);
        Ok(Some(full_path))
    } else {
        // User cancelled
        // User hủy
        Ok(None)
    }
}

/// Check if package name should trigger multi-registry search
/// Kiểm tra tên package có nên trigger search đa registry không
fn should_trigger_search(package_name: &str) -> bool {
    // Don't search if:
    // - Has version specifier (@1.0.0, ^1.0.0)
    // - Has scope (@types/node, @angular/core)
    // - Is a path (./local, ../sibling, file:...)
    // - Is a URL (http://, https://, git://)
    
    if package_name.contains('@') && !package_name.starts_with('@') {
        return false; // Has version
    }
    
    if package_name.starts_with('.') || package_name.starts_with("file:") {
        return false; // Local path
    }
    
    if package_name.starts_with("http://")
        || package_name.starts_with("https://")
        || package_name.starts_with("git://")
        || package_name.contains("github.com/")
    {
        return false; // URL
    }
    
    // For scoped packages (@org/pkg), only search if no version
    // Cho packages có scope, chỉ search nếu không có version
    if package_name.starts_with('@') {
        return !package_name.contains('/') || package_name.ends_with('/');
    }
    
    true
}

/// Detect project context from current directory
/// Phát hiện context dự án từ thư mục hiện tại
fn detect_project_context(project_root: &Path) -> Result<ProjectContext> {
    let mut signatures = Vec::new();
    let mut core = "web".to_string(); // Default
    
    // Check for signature files
    // Kiểm tra file signature
    if project_root.join("package.json").exists() {
        signatures.push("package.json".to_string());
        core = "web".to_string();
    }
    if project_root.join("Cargo.toml").exists() {
        signatures.push("Cargo.toml".to_string());
        core = "lib".to_string();
    }
    if project_root.join("go.mod").exists() {
        signatures.push("go.mod".to_string());
        core = "cloud".to_string();
    }
    if project_root.join("requirements.txt").exists()
        || project_root.join("pyproject.toml").exists()
    {
        signatures.push("requirements.txt".to_string());
        core = "ai".to_string();
    }
    if project_root.join("pubspec.yaml").exists() {
        signatures.push("pubspec.yaml".to_string());
        core = "app".to_string();
    }
    
    Ok(ProjectContext { core, signatures })
}

/// Format package path based on registry
/// Format đường dẫn package theo registry
fn format_package_path(name: &str, registry: mgc_search::Registry) -> String {
    use mgc_search::Registry;
    
    match registry {
        Registry::Npm => name.to_string(),
        Registry::Crates => name.to_string(),
        Registry::Go => name.to_string(), // Go uses full path (github.com/...)
        Registry::PyPI => name.to_string(),
    }
}


#[cfg(test)]
#[path = "../../test/search_helper_test.rs"]
mod tests;
