//! OIDC claim set — standard claims plus the CI fields registries bind.
//! (Claim OIDC — chuẩn + field CI để registry ràng buộc.)

use serde::{Deserialize, Serialize};

/// Audience may be a single string or a list (both occur in the wild).
/// Audience có thể là một chuỗi hoặc danh sách chuỗi.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Audience {
    Single(String),
    Multiple(Vec<String>),
}

impl Audience {
    /// True when `audience` is listed.
    /// Trả về true nếu audience nằm trong giá trị đã khai báo.
    pub fn contains(&self, audience: &str) -> bool {
        match self {
            Self::Single(single) => single == audience,
            Self::Multiple(list) => list.iter().any(|entry| entry == audience),
        }
    }
}

/// Verified OIDC identity of one CI workload.
/// Danh tính OIDC đã xác minh của một workload CI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OidcClaims {
    /// Issuer (`https://token.actions.githubusercontent.com`, …).
    /// Đơn vị phát hành token.
    pub iss: String,
    /// Subject — the workload identity the registry binds
    /// (e.g. `repo:octo/pkg:ref:refs/heads/main`).
    /// Subject — danh tính workload mà registry dùng để đối chiếu binding.
    pub sub: String,
    /// Intended audience(s).
    /// Audience dự kiến của token.
    pub aud: Audience,
    /// Expiry (unix seconds).
    /// Thời điểm hết hạn, tính bằng giây Unix.
    pub exp: u64,
    /// Issued-at (unix seconds).
    /// Thời điểm phát hành, tính bằng giây Unix.
    pub iat: u64,
    /// GitHub repository (`octo/pkg`), when present.
    /// Repository GitHub, nếu token có claim này.
    #[serde(default)]
    pub repository: Option<String>,
    /// Resolved reusable workflow ref used in Fulcio's GitHub certificate identity.
    /// Tham chiếu workflow dùng lại mà Fulcio đưa vào định danh chứng thư GitHub.
    #[serde(default)]
    pub job_workflow_ref: Option<String>,
    /// Workflow ref (`octo/pkg/.github/workflows/publish.yml@refs/heads/main`).
    /// Tham chiếu workflow, nếu token có claim này.
    #[serde(default)]
    pub workflow_ref: Option<String>,
    /// Trigger event (`push`, `workflow_dispatch`, …).
    /// Sự kiện kích hoạt workflow, nếu token có claim này.
    #[serde(default)]
    pub event_name: Option<String>,
}

impl OidcClaims {
    /// Compare identity claims across registry and Sigstore audiences.
    /// So sánh claim định danh giữa token audience registry và Sigstore.
    pub fn same_workload_identity(&self, other: &Self) -> bool {
        self.iss == other.iss
            && self.sub == other.sub
            && self.repository == other.repository
            && self.workflow_ref == other.workflow_ref
            && self.job_workflow_ref == other.job_workflow_ref
            && self.event_name == other.event_name
    }
}
