//! Summary-crate error type.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SummaryError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("no usable laps: {0}")]
    NoData(String),
}

pub type Result<T> = std::result::Result<T, SummaryError>;
