//! Real-time tier error type.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RtError {
    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),

    #[error("audio error: {0}")]
    Audio(String),
}

pub type Result<T> = std::result::Result<T, RtError>;
