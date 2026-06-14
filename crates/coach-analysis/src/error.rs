//! Analysis error type.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error("not enough data: {0}")]
    InsufficientData(String),

    #[error("invalid input: {0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, AnalysisError>;
