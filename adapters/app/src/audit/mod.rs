//! `audit/mod.rs` — Security audit for app adapter.

pub mod scanner;

use crate::language::AppLanguage;
use mgc_types::MgResult;
use mgc_types::adapter::AuditReport;
use std::path::Path;

pub async fn run_audit(language: AppLanguage, project_root: &Path) -> MgResult<AuditReport> {
    match language {
        AppLanguage::Flutter => scanner::audit_flutter(project_root).await,
        AppLanguage::Kotlin => scanner::audit_kotlin(project_root).await,
        AppLanguage::Swift => scanner::audit_swift(project_root).await,
        // React Native is the aggregate case BY CONTRACT (Tech Lead §3:
        // Node + Gradle + Pods/native in ONE report) — audit_multi
        // merges every manifest it recognizes, and the JS side joins
        // via the npm bulk advisory flow when package.json pins exist.
        // React Native là trường hợp aggregate THEO HỢP ĐỒNG (Node +
        // Gradle + Pods/native trong MỘT report) — audit_multi gộp mọi
        // manifest nhận diện, phần JS vào qua npm bulk advisory khi có
        // ghim package.json.
        AppLanguage::ReactNative | AppLanguage::Multi => scanner::audit_multi(project_root).await,
        AppLanguage::ObjC => scanner::audit_cocoapods(project_root).await,
    }
}
