//! Identifier types used as keys throughout the system.
//!
//! References are keyed on `(track_id, car_id)` (spec §5); laps and sessions get
//! their own ids. These are transparent `String` newtypes — they serialize as
//! plain strings but prevent accidentally mixing, say, a `TrackId` with a
//! `CarId` at a call site.

use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! string_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(s: impl Into<String>) -> Self {
                Self(s.into())
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_owned())
            }
        }
    };
}

string_id!(
    /// Track identity, from the AC `Static` page `track` field.
    TrackId
);
string_id!(
    /// Car identity, from the AC `Static` page `carModel` field.
    CarId
);
string_id!(
    /// One recorded lap.
    LapId
);
string_id!(
    /// A session groups laps for the post-event summary (spec §10).
    SessionId
);

/// Segment index within a track's ordered segment list (spec §12.1).
pub type SegmentId = u32;

/// The `(track, car)` pair every reference, action point, and plan is keyed on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackCar {
    pub track_id: TrackId,
    pub car_id: CarId,
}

impl TrackCar {
    pub fn new(track_id: impl Into<TrackId>, car_id: impl Into<CarId>) -> Self {
        Self {
            track_id: track_id.into(),
            car_id: car_id.into(),
        }
    }
}
