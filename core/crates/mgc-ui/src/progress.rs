/// Progress tracking utilities
use indicatif::{MultiProgress, ProgressBar};

/// Multi-progress manager for concurrent operations
pub struct ProgressManager {
    multi: MultiProgress,
}

impl ProgressManager {
    pub fn new() -> Self {
        Self {
            multi: MultiProgress::new(),
        }
    }

    pub fn add(&self, pb: ProgressBar) -> ProgressBar {
        self.multi.add(pb)
    }
}

impl Default for ProgressManager {
    fn default() -> Self {
        Self::new()
    }
}
