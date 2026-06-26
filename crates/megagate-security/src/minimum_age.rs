use chrono::{Utc, Duration};
use megagate_types::error::{MegagateError, Result};
use megagate_types::package::LockedPackage;

pub struct MinimumReleaseAge {
    min_hours: u32,
}

impl MinimumReleaseAge {
    pub fn new(min_hours: u32) -> Self {
        Self { min_hours }
    }

    pub fn check(&self, pkg: &LockedPackage) -> Result<()> {
        if let Some(publish_time) = pkg.publish_time {
            let now = Utc::now();
            let age = now.signed_duration_since(publish_time);
            let min_age = Duration::hours(self.min_hours as i64);

            if age < min_age {
                return Err(MegagateError::SecurityViolation(format!(
                    "Package {}@{} was published {} ago (minimum: {} hours)",
                    pkg.name,
                    pkg.version,
                    humantime::format_duration(age.to_std().unwrap()),
                    self.min_hours
                )));
            }
        }
        Ok(())
    }
}

impl Default for MinimumReleaseAge {
    fn default() -> Self {
        Self::new(24)
    }
}