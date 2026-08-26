//! `audit/mod.rs` — Security audit for app adapter.

pub mod scanner;

use crate::language::AppLanguage;
use mgc_types::adapter::AuditReport;
use mgc_types::{MgError, MgResult};
use std::path::Path;

pub async fn run_audit(language: AppLanguage, project_root: &Path) -> MgResult<AuditReport> {
    match language {
        AppLanguage::Flutter => scanner::audit_flutter(project_root).await,
        AppLanguage::Kotlin => scanner::audit_kotlin(project_root).await,
        AppLanguage::Swift => scanner::audit_swift(project_root).await,
        AppLanguage::ReactNative => Err(MgError::Other(
            "React Native audit should delegate to web adapter".to_string(),
        )),
        AppLanguage::ObjC => scanner::audit_cocoapods(project_root).await,
        AppLanguage::Multi => scanner::audit_multi(project_root).await,
    }
}
