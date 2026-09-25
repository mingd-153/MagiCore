//! AI core audit — dispatch to the adapter's real scanner via the shared
//! pipeline (detect → adapter → scanner → typed report → exit contract).
//! Audit core AI — dispatch sang scanner thật của adapter qua pipeline
//! chung; CLI không chứa business logic (Tech Lead §11).

use super::{OutputFormat, StrictMode, finish_and_print, run_adapter_audit};
use crate::context::ProjectContext;

pub(crate) async fn audit(ctx: &ProjectContext, fmt: OutputFormat) -> anyhow::Result<()> {
    let report = run_adapter_audit(ctx).await?;
    finish_and_print("ai", &report, StrictMode::from_env(), fmt).await
}
