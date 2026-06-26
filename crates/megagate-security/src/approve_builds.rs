use megagate_types::package::LockedPackage;
use std::collections::HashSet;

pub struct ApproveBuilds {
    approved: HashSet<String>,
}

impl ApproveBuilds {
    pub fn new() -> Self {
        Self {
            approved: HashSet::new(),
        }
    }

    pub fn load_from_lockfile(&mut self, approved: Vec<String>) {
        self.approved = approved.into_iter().collect();
    }

    pub fn is_approved(&self, pkg: &LockedPackage, script: &str) -> bool {
        self.approved.contains(&format!("{}@{}", pkg.name, pkg.version))
            || self.approved.contains(&format!("{}@{}#{}", pkg.name, pkg.version, script))
    }

    pub fn approve(&mut self, pkg: &LockedPackage, script: Option<&str>) {
        if let Some(script) = script {
            self.approved.insert(format!("{}@{}#{}", pkg.name, pkg.version, script));
        } else {
            self.approved.insert(format!("{}@{}", pkg.name, pkg.version));
        }
    }

    pub fn get_approved_list(&self) -> Vec<String> {
        self.approved.iter().cloned().collect()
    }
}

impl Default for ApproveBuilds {
    fn default() -> Self {
        Self::new()
    }
}