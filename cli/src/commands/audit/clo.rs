//! Cloud core audit — dispatch to the adapter's real scanner via the
//! shared pipeline (detect → adapter → scanner → typed report → exit
//! contract).
//! Audit core Cloud — dispatch sang scanner thật của adapter qua pipeline
//! chung; CLI không chứa business logic (Tech Lead §11).

use super::{StrictMode, finish_and_print, run_adapter_audit};
use crate::context::ProjectContext;

pub async fn audit(ctx: &ProjectContext) -> anyhow::Result<()> {
    let report = run_adapter_audit(ctx).await?;
    finish_and_print("clo", &report, StrictMode::from_env()).await
}
