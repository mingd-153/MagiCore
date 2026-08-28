//! Interactive prompt for package selection
//! Prompt tương tác để chọn package

use crate::types::{Registry, SearchResult};
use anyhow::Result;
use std::io::{self, Write};

/// Display search results and prompt user to select
/// Hiển thị kết quả tìm kiếm và prompt user chọn
///
/// Returns selected SearchResult or None if cancelled
/// Trả về SearchResult được chọn hoặc None nếu hủy
pub fn prompt_selection(results: &[SearchResult]) -> Result<Option<SearchResult>> {
    if results.is_empty() {
        println!("\n❌ No packages found.");
        return Ok(None);
    }

    // Display results
    // Hiển thị kết quả
    println!("\n🔍 Found {} package(s):\n", results.len());

    for (idx, result) in results.iter().enumerate() {
        let registry_icon = match result.registry {
            Registry::Npm => "📦",
            Registry::Crates => "🦀",
            Registry::Go => "🐹",
            Registry::PyPI => "🐍",
        };

        println!(
            "  {}. {} {} v{} ({})",
            idx + 1,
            registry_icon,
            result.name,
            result.version,
            result.registry
        );

        // Show description if available
        // Hiển thị mô tả nếu có
        if !result.description.is_empty() {
            println!("     {}", truncate(&result.description, 70));
        }

        // Show metadata
        // Hiển thị metadata
        print_metadata(result);

        println!(); // Blank line between results
    }

    // Prompt for selection
    // Prompt chọn
    print!("\n➤ Select package (1-{}, or 0 to cancel): ", results.len());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let choice = input.trim().parse::<usize>().unwrap_or(0);

    if choice == 0 {
        println!("❌ Cancelled.");
        return Ok(None);
    }

    if choice > results.len() {
        anyhow::bail!("Invalid selection: {}", choice);
    }

    let selected = results[choice - 1].clone();
    println!("\n✅ Selected: {} v{}", selected.name, selected.version);

    Ok(Some(selected))
}

/// Print metadata line (downloads, stars, updated)
/// In dòng metadata (lượt tải, stars, cập nhật)
fn print_metadata(result: &SearchResult) {
    let mut meta_parts = Vec::new();

    if let Some(downloads) = result.metadata.downloads {
        meta_parts.push(format!("↓ {}", format_number(downloads)));
    }

    if let Some(stars) = result.metadata.stars {
        meta_parts.push(format!("⭐ {}", format_number(stars)));
    }

    if !result.metadata.updated.is_empty() {
        meta_parts.push(format!("🕒 {}", result.metadata.updated));
    }

    if let Some(quality) = result.metadata.quality {
        meta_parts.push(format!("✨ {:.0}%", quality));
    }

    if !meta_parts.is_empty() {
        println!("     {}", meta_parts.join(" • "));
    }
}

/// Format large numbers with K/M suffix
/// Format số lớn với hậu tố K/M
fn format_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Truncate string to max length with ellipsis
/// Cắt chuỗi đến độ dài tối đa với dấu ba chấm
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
