//! `rusqlite`-backed [`Store`] implementation.

use crate::error::{Result, StorageError};
use crate::models::{ActionPointsRow, LapRow, SectorBestRow, SegmentTraversalRow, SessionRow};
use crate::schema::SCHEMA_SQL;
use crate::store::Store;
use chrono::{DateTime, Utc};
use coach_core::action_points::ActionPoints;
use coach_core::ids::{LapId, SessionId, TrackCar, TrackId};
use coach_core::segment::{Segment, TrackGeometry};
use coach_core::session::SessionType;
use coach_core::trace::LapTrace;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::Path;

/// Embedded SQLite store. Single file (or `:memory:` for tests).
pub struct SqliteStore {
    conn: Connection,
}

impl SqliteStore {
    /// Open (creating if needed) a database file and apply the schema.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::from_conn(conn)
    }

    /// In-memory store, for tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::from_conn(Connection::open_in_memory()?)
    }

    fn from_conn(conn: Connection) -> Result<Self> {
        conn.execute_batch(SCHEMA_SQL)?;
        Ok(Self { conn })
    }
}

// --- small encoders ---

fn dt_to_str(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

fn str_to_dt(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| StorageError::NotFound(format!("bad timestamp {s:?}: {e}")))
}

fn session_type_str(t: SessionType) -> &'static str {
    match t {
        SessionType::Practice => "practice",
        SessionType::Qualify => "qualify",
        SessionType::Race => "race",
        SessionType::Other => "other",
    }
}

fn session_type_from(s: &str) -> SessionType {
    match s {
        "practice" => SessionType::Practice,
        "qualify" => SessionType::Qualify,
        "race" => SessionType::Race,
        _ => SessionType::Other,
    }
}

fn encode_trace(t: &LapTrace) -> Result<Vec<u8>> {
    Ok(bincode::serialize(t)?)
}

fn decode_trace(bytes: &[u8]) -> Result<LapTrace> {
    Ok(bincode::deserialize(bytes)?)
}

impl Store for SqliteStore {
    fn upsert_track_geometry(&self, geom: &TrackGeometry) -> Result<()> {
        let segments_json = serde_json::to_string(&geom.segments)?;
        self.conn.execute(
            "INSERT INTO track_geometry (track_id, track_length_m, segments)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(track_id) DO UPDATE SET
               track_length_m = excluded.track_length_m,
               segments = excluded.segments",
            params![geom.track_id.as_str(), geom.track_length_m, segments_json],
        )?;
        Ok(())
    }

    fn get_track_geometry(&self, track_id: &TrackId) -> Result<Option<TrackGeometry>> {
        self.conn
            .query_row(
                "SELECT track_length_m, segments FROM track_geometry WHERE track_id = ?1",
                params![track_id.as_str()],
                |row| {
                    let length: f64 = row.get(0)?;
                    let segments_json: String = row.get(1)?;
                    Ok((length, segments_json))
                },
            )
            .optional()?
            .map(|(length, segments_json)| {
                let segments: Vec<Segment> = serde_json::from_str(&segments_json)?;
                Ok(TrackGeometry {
                    track_id: track_id.clone(),
                    track_length_m: length,
                    segments,
                })
            })
            .transpose()
    }

    fn upsert_action_points(&self, row: &ActionPointsRow) -> Result<()> {
        let p = &row.points;
        self.conn.execute(
            "INSERT INTO action_points
               (track_id, car_id, segment_id, brake_point_dist_m, turn_in_dist_m,
                apex_dist_m, throttle_on_dist_m)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(track_id, car_id, segment_id) DO UPDATE SET
               brake_point_dist_m = excluded.brake_point_dist_m,
               turn_in_dist_m = excluded.turn_in_dist_m,
               apex_dist_m = excluded.apex_dist_m,
               throttle_on_dist_m = excluded.throttle_on_dist_m",
            params![
                row.track_id.as_str(),
                row.car_id.as_str(),
                p.segment_id,
                p.brake_point_dist_m,
                p.turn_in_dist_m,
                p.apex_dist_m,
                p.throttle_on_dist_m,
            ],
        )?;
        Ok(())
    }

    fn get_action_points(&self, key: &TrackCar) -> Result<Vec<ActionPointsRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT segment_id, brake_point_dist_m, turn_in_dist_m, apex_dist_m, throttle_on_dist_m
             FROM action_points WHERE track_id = ?1 AND car_id = ?2 ORDER BY segment_id",
        )?;
        let rows = stmt
            .query_map(params![key.track_id.as_str(), key.car_id.as_str()], |row| {
                Ok(ActionPointsRow {
                    track_id: key.track_id.clone(),
                    car_id: key.car_id.clone(),
                    points: ActionPoints {
                        segment_id: row.get(0)?,
                        brake_point_dist_m: row.get(1)?,
                        turn_in_dist_m: row.get(2)?,
                        apex_dist_m: row.get(3)?,
                        throttle_on_dist_m: row.get(4)?,
                    },
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    fn upsert_session(&self, row: &SessionRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sessions
               (session_id, track_id, car_id, session_type, started_at, ended_at,
                summary_md_path, summary_json_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(session_id) DO UPDATE SET
               ended_at = excluded.ended_at,
               summary_md_path = excluded.summary_md_path,
               summary_json_path = excluded.summary_json_path",
            params![
                row.session_id.as_str(),
                row.track_id.as_str(),
                row.car_id.as_str(),
                session_type_str(row.session_type),
                dt_to_str(&row.started_at),
                row.ended_at.as_ref().map(dt_to_str),
                row.summary_md_path,
                row.summary_json_path,
            ],
        )?;
        Ok(())
    }

    fn get_session(&self, session_id: &SessionId) -> Result<Option<SessionRow>> {
        self.conn
            .query_row(
                "SELECT track_id, car_id, session_type, started_at, ended_at,
                        summary_md_path, summary_json_path
                 FROM sessions WHERE session_id = ?1",
                params![session_id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                    ))
                },
            )
            .optional()?
            .map(|(track, car, stype, started, ended, md, json)| {
                Ok(SessionRow {
                    session_id: session_id.clone(),
                    track_id: track.into(),
                    car_id: car.into(),
                    session_type: session_type_from(&stype),
                    started_at: str_to_dt(&started)?,
                    ended_at: ended.as_deref().map(str_to_dt).transpose()?,
                    summary_md_path: md,
                    summary_json_path: json,
                })
            })
            .transpose()
    }

    fn insert_lap(&self, row: &LapRow) -> Result<()> {
        let trace_blob = row.trace.as_ref().map(encode_trace).transpose()?;
        self.conn.execute(
            "INSERT INTO laps
               (lap_id, session_id, track_id, car_id, lap_time_ms, is_valid,
                is_in_out_lap, is_outlier, recorded_at, trace)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                row.lap_id.as_str(),
                row.session_id.as_str(),
                row.track_id.as_str(),
                row.car_id.as_str(),
                row.lap_time_ms,
                row.is_valid as i64,
                row.is_in_out_lap as i64,
                row.is_outlier as i64,
                dt_to_str(&row.recorded_at),
                trace_blob,
            ],
        )?;
        Ok(())
    }

    fn laps_for_session(&self, session_id: &SessionId, with_trace: bool) -> Result<Vec<LapRow>> {
        let sql = if with_trace {
            "SELECT lap_id, track_id, car_id, lap_time_ms, is_valid, is_in_out_lap,
                    is_outlier, recorded_at, trace
             FROM laps WHERE session_id = ?1 ORDER BY recorded_at"
        } else {
            "SELECT lap_id, track_id, car_id, lap_time_ms, is_valid, is_in_out_lap,
                    is_outlier, recorded_at, NULL
             FROM laps WHERE session_id = ?1 ORDER BY recorded_at"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt
            .query_map(params![session_id.as_str()], |row| {
                Ok(lap_row_from(session_id.clone(), row))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().collect()
    }

    fn insert_segment_traversal(&self, row: &SegmentTraversalRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO segment_traversals
               (lap_id, segment_id, time_ms, is_valid, entry_speed_kmh,
                brake_point_dist_m, turn_in_dist_m, apex_speed_kmh, min_speed_kmh,
                throttle_on_dist_m)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(lap_id, segment_id) DO UPDATE SET
               time_ms = excluded.time_ms,
               is_valid = excluded.is_valid,
               entry_speed_kmh = excluded.entry_speed_kmh,
               brake_point_dist_m = excluded.brake_point_dist_m,
               turn_in_dist_m = excluded.turn_in_dist_m,
               apex_speed_kmh = excluded.apex_speed_kmh,
               min_speed_kmh = excluded.min_speed_kmh,
               throttle_on_dist_m = excluded.throttle_on_dist_m",
            params![
                row.lap_id.as_str(),
                row.segment_id,
                row.time_ms,
                row.is_valid as i64,
                row.entry_speed_kmh,
                row.brake_point_dist_m,
                row.turn_in_dist_m,
                row.apex_speed_kmh,
                row.min_speed_kmh,
                row.throttle_on_dist_m,
            ],
        )?;
        Ok(())
    }

    fn segment_traversals_for_session(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<SegmentTraversalRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT st.lap_id, st.segment_id, st.time_ms, st.is_valid, st.entry_speed_kmh,
                    st.brake_point_dist_m, st.turn_in_dist_m, st.apex_speed_kmh,
                    st.min_speed_kmh, st.throttle_on_dist_m
             FROM segment_traversals st
             JOIN laps l ON l.lap_id = st.lap_id
             WHERE l.session_id = ?1
             ORDER BY l.recorded_at, st.segment_id",
        )?;
        let rows = stmt
            .query_map(params![session_id.as_str()], traversal_from)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    fn segment_traversals_for_lap(&self, lap_id: &LapId) -> Result<Vec<SegmentTraversalRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT lap_id, segment_id, time_ms, is_valid, entry_speed_kmh,
                    brake_point_dist_m, turn_in_dist_m, apex_speed_kmh, min_speed_kmh,
                    throttle_on_dist_m
             FROM segment_traversals WHERE lap_id = ?1 ORDER BY segment_id",
        )?;
        let rows = stmt
            .query_map(params![lap_id.as_str()], traversal_from)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    fn upsert_sector_best(&self, row: &SectorBestRow) -> Result<()> {
        let trace_blob = encode_trace(&row.trace)?;
        self.conn.execute(
            "INSERT INTO sector_bests
               (track_id, car_id, segment_id, best_time_ms, source_lap_id,
                entry_speed_kmh, trace, time_variance_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(track_id, car_id, segment_id) DO UPDATE SET
               best_time_ms = excluded.best_time_ms,
               source_lap_id = excluded.source_lap_id,
               entry_speed_kmh = excluded.entry_speed_kmh,
               trace = excluded.trace,
               time_variance_ms = excluded.time_variance_ms",
            params![
                row.track_id.as_str(),
                row.car_id.as_str(),
                row.segment_id,
                row.best_time_ms,
                row.source_lap_id.as_str(),
                row.entry_speed_kmh,
                trace_blob,
                row.time_variance_ms,
            ],
        )?;
        Ok(())
    }

    fn sector_bests(&self, key: &TrackCar) -> Result<Vec<SectorBestRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT segment_id, best_time_ms, source_lap_id, entry_speed_kmh, trace, time_variance_ms
             FROM sector_bests WHERE track_id = ?1 AND car_id = ?2 ORDER BY segment_id",
        )?;
        let rows = stmt
            .query_map(params![key.track_id.as_str(), key.car_id.as_str()], |row| {
                let trace_bytes: Vec<u8> = row.get(4)?;
                Ok((
                    row.get::<_, u32>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, f64>(3)?,
                    trace_bytes,
                    row.get::<_, f64>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(seg, best, src, entry, bytes, var)| {
                Ok(SectorBestRow {
                    track_id: key.track_id.clone(),
                    car_id: key.car_id.clone(),
                    segment_id: seg,
                    best_time_ms: best,
                    source_lap_id: src.into(),
                    entry_speed_kmh: entry,
                    trace: decode_trace(&bytes)?,
                    time_variance_ms: var,
                })
            })
            .collect()
    }
}

/// Build a [`LapRow`] from a query row laid out as
/// `(lap_id, track_id, car_id, lap_time_ms, is_valid, is_in_out_lap, is_outlier,
/// recorded_at, trace?)`. Returns a `Result` so trace/timestamp decode errors
/// surface; `query_map` wraps it, and the caller flattens.
fn lap_row_from(session_id: SessionId, row: &Row<'_>) -> Result<LapRow> {
    let trace_bytes: Option<Vec<u8>> = row.get(8)?;
    let recorded_at: String = row.get(7)?;
    Ok(LapRow {
        lap_id: row.get::<_, String>(0)?.into(),
        session_id,
        track_id: row.get::<_, String>(1)?.into(),
        car_id: row.get::<_, String>(2)?.into(),
        lap_time_ms: row.get(3)?,
        is_valid: row.get::<_, i64>(4)? != 0,
        is_in_out_lap: row.get::<_, i64>(5)? != 0,
        is_outlier: row.get::<_, i64>(6)? != 0,
        recorded_at: str_to_dt(&recorded_at)?,
        trace: trace_bytes.as_deref().map(decode_trace).transpose()?,
    })
}

fn traversal_from(row: &Row<'_>) -> rusqlite::Result<SegmentTraversalRow> {
    Ok(SegmentTraversalRow {
        lap_id: row.get::<_, String>(0)?.into(),
        segment_id: row.get(1)?,
        time_ms: row.get(2)?,
        is_valid: row.get::<_, i64>(3)? != 0,
        entry_speed_kmh: row.get(4)?,
        brake_point_dist_m: row.get(5)?,
        turn_in_dist_m: row.get(6)?,
        apex_speed_kmh: row.get(7)?,
        min_speed_kmh: row.get(8)?,
        throttle_on_dist_m: row.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use coach_core::segment::{SegmentType, TrackGeometry};

    #[test]
    fn geometry_round_trips() {
        let store = SqliteStore::open_in_memory().unwrap();
        let geom = TrackGeometry {
            track_id: "brands_hatch".into(),
            track_length_m: 3916.0,
            segments: vec![Segment {
                segment_id: 0,
                seg_type: SegmentType::Straight,
                start_dist_m: 0.0,
                end_dist_m: 200.0,
            }],
        };
        store.upsert_track_geometry(&geom).unwrap();
        let got = store
            .get_track_geometry(&"brands_hatch".into())
            .unwrap()
            .unwrap();
        assert_eq!(got, geom);
    }

    #[test]
    fn lap_and_traversal_query_by_session() {
        let store = SqliteStore::open_in_memory().unwrap();
        let sid: SessionId = "s1".into();
        store
            .upsert_session(&SessionRow {
                session_id: sid.clone(),
                track_id: "t".into(),
                car_id: "c".into(),
                session_type: SessionType::Practice,
                started_at: Utc::now(),
                ended_at: None,
                summary_md_path: None,
                summary_json_path: None,
            })
            .unwrap();
        store
            .insert_lap(&LapRow {
                lap_id: "l1".into(),
                session_id: sid.clone(),
                track_id: "t".into(),
                car_id: "c".into(),
                lap_time_ms: 99_250,
                is_valid: true,
                is_in_out_lap: false,
                is_outlier: false,
                recorded_at: Utc::now(),
                trace: None,
            })
            .unwrap();
        store
            .insert_segment_traversal(&SegmentTraversalRow {
                lap_id: "l1".into(),
                segment_id: 5,
                time_ms: 4200,
                is_valid: true,
                entry_speed_kmh: 210.0,
                brake_point_dist_m: Some(1840.0),
                turn_in_dist_m: None,
                apex_speed_kmh: Some(120.0),
                min_speed_kmh: Some(118.0),
                throttle_on_dist_m: None,
            })
            .unwrap();

        let laps = store.laps_for_session(&sid, false).unwrap();
        assert_eq!(laps.len(), 1);
        assert_eq!(laps[0].lap_time_ms, 99_250);

        let trav = store.segment_traversals_for_session(&sid).unwrap();
        assert_eq!(trav.len(), 1);
        assert_eq!(trav[0].segment_id, 5);
        assert_eq!(trav[0].brake_point_dist_m, Some(1840.0));
    }
}
