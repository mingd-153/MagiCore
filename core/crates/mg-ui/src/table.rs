/// Table formatting utilities
use console::{style, pad_str, Alignment};

/// Simple table for displaying data
pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new(headers: Vec<String>) -> Self {
        Self {
            headers,
            rows: Vec::new(),
        }
    }

    pub fn add_row(&mut self, row: Vec<String>) {
        self.rows.push(row);
    }

    pub fn print(&self) {
        // Print headers
        let header_str = self.headers
            .iter()
            .map(|h| style(h).bold().to_string())
            .collect::<Vec<_>>()
            .join("  ");
        println!("{}", header_str);

        // Print separator
        println!("{}", "─".repeat(80));

        // Print rows
        for row in &self.rows {
            println!("{}", row.join("  "));
        }
    }
}
