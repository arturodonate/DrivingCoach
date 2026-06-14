//! # coach-summary
//!
//! The deterministic post-event performance summary (spec §10, build step
//! §13.7). [`build_summary`] computes the summary from persisted laps, segment
//! traversals, and accumulated diagnoses (no model); [`render_markdown`] renders
//! the human-readable `.md`, and the model serializes to the machine-readable
//! `.json`. [`write_artifacts`] writes both, named
//! `{track}_{car}_{session_type}_{timestamp}`.

pub mod builder;
pub mod error;
pub mod markdown;
pub mod model;
pub mod stats;

pub use builder::{build_summary, SummaryInputs};
pub use error::{Result, SummaryError};
pub use markdown::render_markdown;
pub use model::Summary;

use std::path::{Path, PathBuf};

/// Replace anything outside `[A-Za-z0-9.-]` with `_` so the name is filesystem-
/// safe on Windows and Unix alike.
fn slug(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Write both artifacts to `dir`, returning `(md_path, json_path)`.
pub fn write_artifacts(summary: &Summary, dir: impl AsRef<Path>) -> Result<(PathBuf, PathBuf)> {
    std::fs::create_dir_all(&dir)?;
    let stem = format!(
        "{}_{}_{}_{}",
        slug(&summary.track),
        slug(&summary.car),
        slug(summary.session_type.label()),
        slug(&summary.generated_at),
    );
    let md_path = dir.as_ref().join(format!("{stem}.md"));
    let json_path = dir.as_ref().join(format!("{stem}.json"));
    std::fs::write(&md_path, render_markdown(summary))?;
    std::fs::write(&json_path, serde_json::to_string_pretty(summary)?)?;
    Ok((md_path, json_path))
}
