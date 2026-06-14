//! Cue-crate error type.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CueError {
    #[error("audio I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Piper synthesis failed: {0}")]
    Synthesis(String),

    #[error("malformed WAV: {0}")]
    Wav(String),
}

pub type Result<T> = std::result::Result<T, CueError>;
