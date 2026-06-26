use megagate_types::error::Result;
use megagate_types::package::ProvenanceInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceAttestation {
    pub package_name: String,
    pub version: String,
    pub repository_url: String,
    pub commit_hash: String,
    pub builder_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub signature: Option<String>,
}

pub struct ProvenanceVerifier {
    trusted_builders: Vec<String>,
}

impl ProvenanceVerifier {
    pub fn new(trusted_builders: Vec<String>) -> Self {
        Self { trusted_builders }
    }

    pub fn verify(&self, provenance: &ProvenanceInfo) -> Result<bool> {
        if provenance.repository_url.is_none() {
            return Ok(false);
        }
        if provenance.commit_hash.is_none() {
            return Ok(false);
        }
        if provenance.builder_id.is_none() {
            return Ok(false);
        }
        if provenance.signature.is_none() {
            return Ok(false);
        }

        let builder = provenance.builder_id.as_ref().unwrap();
        Ok(self.trusted_builders.contains(builder))
    }

    pub fn generate_attestation(
        &self,
        package_name: &str,
        version: &str,
        repo_url: &str,
        commit: &str,
        builder: &str,
    ) -> ProvenanceAttestation {
        ProvenanceAttestation {
            package_name: package_name.to_string(),
            version: version.to_string(),
            repository_url: repo_url.to_string(),
            commit_hash: commit.to_string(),
            builder_id: builder.to_string(),
            timestamp: chrono::Utc::now(),
            signature: None,
        }
    }
}

impl Default for ProvenanceVerifier {
    fn default() -> Self {
        Self::new(vec!["github-actions".to_string(), "gitlab-ci".to_string()])
    }
}