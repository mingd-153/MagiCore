use crate::graph::Conflict;
use megagate_types::error::MegagateError;
use megagate_fetcher::registry_client::RegistryClient;
use semver::Version;

#[derive(Debug, Clone)]
pub struct ResolutionDecision {
    pub name: String,
    pub chosen_version: Version,
    pub strategy: ResolutionStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionStrategy {
    Hoisted,
    Duplicated,
    Locked,
    Workspace,
}

pub async fn resolve_conflicts(
    conflicts: Vec<Conflict>,
    _registry_client: &dyn RegistryClient,
) -> Result<Vec<ResolutionDecision>, MegagateError> {
    let mut decisions = Vec::new();

    for conflict in conflicts {
        let versions: Vec<Version> = conflict
            .versions
            .iter()
            .map(|v| v.version.clone())
            .collect();

        if let Some(highest_compatible) = find_highest_compatible(&versions) {
            decisions.push(ResolutionDecision {
                name: conflict.name.clone(),
                chosen_version: highest_compatible,
                strategy: ResolutionStrategy::Hoisted,
            });
        } else {
            decisions.push(ResolutionDecision {
                name: conflict.name,
                chosen_version: versions[0].clone(),
                strategy: ResolutionStrategy::Duplicated,
            });
        }
    }

    Ok(decisions)
}

fn find_highest_compatible(versions: &[Version]) -> Option<Version> {
    if versions.is_empty() {
        return None;
    }
    let mut sorted = versions.to_vec();
    sorted.sort_by(|a, b| b.cmp(a));
    let highest = sorted[0].clone();
    Some(highest)
}