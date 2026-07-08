pub mod serialization;

use mg_types::PackageId;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LockPackage {
    pub id: PackageId,
    pub integrity: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Lockfile {
    pub packages: Vec<LockPackage>,
}
