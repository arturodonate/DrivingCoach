//! Error type for the telemetry crate.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TelemetryError {
    #[error("telemetry I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("recording (de)serialization error: {0}")]
    Bincode(#[from] bincode::Error),

    #[cfg(windows)]
    #[error("shared-memory error: {0}")]
    SharedMemory(String),
}
