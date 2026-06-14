//! Database schema (spec §12.1), as SQLite DDL.
//!
//! Types are mapped to SQLite affinities: `DOUBLE`→`REAL`, `TIMESTAMP`→`TEXT`
//! (RFC 3339), `JSON`/`BLOB` stored as `TEXT`/`BLOB`. The table and column names
//! follow the spec verbatim.

/// Applied once at open; `IF NOT EXISTS` makes it idempotent.
pub const SCHEMA_SQL: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS track_geometry (
    track_id        TEXT PRIMARY KEY,
    track_length_m  REAL NOT NULL,
    segments        TEXT NOT NULL          -- JSON: ordered [{segment_id,type,start_dist_m,end_dist_m}]
);

CREATE TABLE IF NOT EXISTS action_points (
    track_id            TEXT NOT NULL,
    car_id              TEXT NOT NULL,
    segment_id          INTEGER NOT NULL,
    brake_point_dist_m  REAL,
    turn_in_dist_m      REAL,
    apex_dist_m         REAL,
    throttle_on_dist_m  REAL,
    PRIMARY KEY (track_id, car_id, segment_id)
);

CREATE TABLE IF NOT EXISTS sessions (
    session_id        TEXT PRIMARY KEY,
    track_id          TEXT NOT NULL,
    car_id            TEXT NOT NULL,
    session_type      TEXT NOT NULL,       -- practice | qualify | race | other
    started_at        TEXT NOT NULL,
    ended_at          TEXT,
    summary_md_path   TEXT,
    summary_json_path TEXT
);

CREATE TABLE IF NOT EXISTS laps (
    lap_id          TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL,
    track_id        TEXT NOT NULL,
    car_id          TEXT NOT NULL,
    lap_time_ms     INTEGER NOT NULL,
    is_valid        INTEGER NOT NULL,      -- fully clean lap (0/1)
    is_in_out_lap   INTEGER NOT NULL,      -- pit entry/exit lap (0/1)
    is_outlier      INTEGER NOT NULL,      -- traffic/incident-affected (0/1)
    recorded_at     TEXT NOT NULL,
    trace           BLOB                   -- bincode LapTrace on the distance grid
);
CREATE INDEX IF NOT EXISTS idx_laps_session ON laps(session_id);

CREATE TABLE IF NOT EXISTS segment_traversals (
    lap_id              TEXT NOT NULL,
    segment_id          INTEGER NOT NULL,
    time_ms             INTEGER NOT NULL,
    is_valid            INTEGER NOT NULL,  -- clean within this segment (0/1)
    entry_speed_kmh     REAL NOT NULL,
    brake_point_dist_m  REAL,
    turn_in_dist_m      REAL,
    apex_speed_kmh      REAL,
    min_speed_kmh       REAL,
    throttle_on_dist_m  REAL,
    PRIMARY KEY (lap_id, segment_id)
);

CREATE TABLE IF NOT EXISTS sector_bests (
    track_id          TEXT NOT NULL,
    car_id            TEXT NOT NULL,
    segment_id        INTEGER NOT NULL,
    best_time_ms      INTEGER NOT NULL,
    source_lap_id     TEXT NOT NULL,
    entry_speed_kmh   REAL NOT NULL,       -- for the entry-speed guard (§5.2)
    trace             BLOB NOT NULL,       -- this segment's slice of the source lap
    time_variance_ms  REAL NOT NULL,       -- consistency signal (§5.5)
    PRIMARY KEY (track_id, car_id, segment_id)
);
"#;
