//! # coach-storage
//!
//! Embedded persistence (spec §12) behind a [`store::Store`] trait. The shipped
//! implementation is [`sqlite::SqliteStore`] (`rusqlite`, bundled). The trait
//! keeps the engine swappable — the spec names DuckDB as an alternative (§11).
//!
//! Lap and segment traces are stored as `bincode`-encoded BLOBs; the §10 summary
//! is a pure query over `laps` + `segment_traversals` + `sessions` and never
//! re-parses traces.

pub mod error;
pub mod models;
pub mod schema;
pub mod sqlite;
pub mod store;

pub use error::{Result, StorageError};
pub use models::{ActionPointsRow, LapRow, SectorBestRow, SegmentTraversalRow, SessionRow};
pub use sqlite::SqliteStore;
pub use store::Store;
