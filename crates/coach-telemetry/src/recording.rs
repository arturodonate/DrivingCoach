//! Recorded-session format and replay source.
//!
//! Per spec §13.1, the offline pipeline is "developed and tuned entirely against
//! recorded laps." A [`Recording`] is a session's [`StaticInfo`] plus its ordered
//! raw frames, serialized with `bincode`. [`RecordedSource`] replays it through
//! the [`TelemetrySource`] trait so the whole pipeline runs with no live game.

use crate::error::TelemetryError;
use crate::frame::TelemetryFrame;
use crate::source::{StaticInfo, TelemetrySource};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A full recorded session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recording {
    pub static_info: StaticInfo,
    pub frames: Vec<TelemetryFrame>,
}

impl Recording {
    pub fn new(static_info: StaticInfo) -> Self {
        Self {
            static_info,
            frames: Vec::new(),
        }
    }

    pub fn push(&mut self, frame: TelemetryFrame) {
        self.frames.push(frame);
    }

    /// Serialize to a `bincode` file.
    pub fn write_to(&self, path: impl AsRef<Path>) -> Result<(), TelemetryError> {
        let bytes = bincode::serialize(self)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    /// Load a `bincode` recording from disk.
    pub fn read_from(path: impl AsRef<Path>) -> Result<Self, TelemetryError> {
        let bytes = std::fs::read(path)?;
        let rec = bincode::deserialize(&bytes)?;
        Ok(rec)
    }
}

/// Replays a [`Recording`] frame by frame.
pub struct RecordedSource {
    static_info: StaticInfo,
    frames: std::vec::IntoIter<TelemetryFrame>,
}

impl RecordedSource {
    pub fn new(recording: Recording) -> Self {
        Self {
            static_info: recording.static_info,
            frames: recording.frames.into_iter(),
        }
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, TelemetryError> {
        Ok(Self::new(Recording::read_from(path)?))
    }
}

impl TelemetrySource for RecordedSource {
    fn poll(&mut self) -> Option<TelemetryFrame> {
        self.frames.next()
    }

    fn static_info(&self) -> Option<&StaticInfo> {
        Some(&self.static_info)
    }
}
