//! mgc trust anchor — Attest the project core identity
//! mgc trust anchor — Chứng thực identity core của project

/// Execute `mgc trust anchor` — Thực thi `mgc trust anchor`
pub fn execute(re_attest: bool, rotate: bool, key_id: Option<&str>) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let project_root =
        mgc_config::project::ProjectConfig::find_project_root(&cwd).unwrap_or_else(|| cwd.clone());
    let marker = mgc_config::project::ProjectConfig::read_core_marker(&project_root)?
        .ok_or_else(|| crate::error::trust_anchor_no_marker(&project_root))?;
    let globals = mgc_platform::paths::GlobalPaths::new()
        .map_err(|error| crate::error::trust_anchor_no_home(&error))?;
    let keyring_path = mgc_crypto::keyring::Keyring::default_path()
        .map_err(|error| crate::error::trust_anchor_failed(&error))?;

    if rotate && key_id.is_some() {
        return Err(crate::error::trust_anchor_rotate_needs_generated_key());
    }
    let attestation = mgc_config::attestation::attest(
        &project_root,
        &marker,
        &keyring_path,
        &globals.identities,
        key_id,
        re_attest,
        rotate,
    )
    .map_err(|error| crate::error::trust_anchor_failed(&error))?;

    println!("✓ Core attested: {} ({})", marker, project_root.display());
    println!("  Key: {}", attestation.key_id);
    if !attestation.prev_keys.is_empty() {
        println!(
            "  Previous keys kept: {}",
            attestation
                .prev_keys
                .iter()
                .map(|key| key.key_id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    println!("  Attestation lives outside the project — editing the project cannot forge it.");
    Ok(())
}

/// Best-effort attestation for `mgc init`: never fail project creation,
/// warn instead (the read path warns again on use).
/// (Chứng thực best-effort cho `mgc init`: không bao giờ làm hỏng tạo
/// project, chỉ cảnh báo.)
pub fn attest_new_project(project_root: &std::path::Path, core: &str) {
    let identities = match mgc_platform::paths::GlobalPaths::new() {
        Ok(paths) => paths.identities,
        Err(_) => return,
    };
    let keyring_path = match mgc_crypto::keyring::Keyring::default_path() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("WARNING: core attestation skipped ({error}) — run `mgc trust anchor` later");
            return;
        }
    };
    if let Err(error) = mgc_config::attestation::attest(
        project_root,
        core,
        &keyring_path,
        &identities,
        None,
        false,
        false,
    ) {
        eprintln!("WARNING: core attestation skipped ({error}) — run `mgc trust anchor` later");
    }
}
