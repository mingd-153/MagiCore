#[derive(Debug, Default, Clone)]
pub struct StoreReport {
    pub total_packages: u64,
    pub total_size_bytes: u64,
    pub total_projects: u64,
    pub verified: u64,
    pub corrupted_files: Vec<String>,
    pub missing_files: Vec<String>,
    pub unreferenced_packages: Vec<String>,
    pub reclaimable_bytes: u64,
    pub duration_ms: u64,
}

impl StoreReport {
    pub fn is_healthy(&self) -> bool {
        self.corrupted_files.is_empty() && self.missing_files.is_empty()
    }
}
