//! Raw Assetto Corsa shared-memory pages (`#[repr(C)]`).
//!
//! # ⚠️ VERIFY ON THE DEPLOY BOX (spec §4.2)
//!
//! These structs mirror the **classic** documented AC shared-memory layout
//! (`SPageFilePhysics` / `SPageFileGraphic` / `SPageFileStatic`). Exact field
//! names, types, and **offsets vary by AC version and with CSP installed** —
//! e.g. newer Graphics pages carry a multi-car `carCoordinates[60][3]` array,
//! and some fields migrate between pages across versions. **Confirm the layout
//! against the shared-memory reference for the installed AC/CSP version** before
//! trusting live telemetry. The `offset_of!` assertions in the tests document
//! the layout this code expects; if AC changes, they (or live readings) will
//! reveal the drift.
//!
//! This module is Windows-only — the offline pipeline never touches it.
#![cfg(windows)]
#![allow(non_snake_case)]

use crate::frame::{AcStatus, TelemetryFrame};

/// Decode a fixed-size UTF-16 (`wchar_t`) field, trimming at the first NUL.
fn wide_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// Physics page, truncated after `numberOfTyresOut` (the deepest field we read).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PageFilePhysics {
    pub packetId: i32,
    pub gas: f32,
    pub brake: f32,
    pub fuel: f32,
    pub gear: i32,
    pub rpms: i32,
    pub steerAngle: f32,
    pub speedKmh: f32,
    pub velocity: [f32; 3],
    pub accG: [f32; 3],
    pub wheelSlip: [f32; 4],
    pub wheelLoad: [f32; 4],
    pub wheelsPressure: [f32; 4],
    pub wheelAngularSpeed: [f32; 4],
    pub tyreWear: [f32; 4],
    pub tyreDirtyLevel: [f32; 4],
    pub tyreCoreTemperature: [f32; 4],
    pub camberRAD: [f32; 4],
    pub suspensionTravel: [f32; 4],
    pub drs: f32,
    pub tc: f32,
    pub heading: f32,
    pub pitch: f32,
    pub roll: f32,
    pub cgHeight: f32,
    pub carDamage: [f32; 5],
    pub numberOfTyresOut: i32,
}

/// Graphics page, truncated after `penaltyTime`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PageFileGraphic {
    pub packetId: i32,
    pub status: i32,
    pub session: i32,
    pub currentTime: [u16; 15],
    pub lastTime: [u16; 15],
    pub bestTime: [u16; 15],
    pub split: [u16; 15],
    pub completedLaps: i32,
    pub position: i32,
    pub iCurrentTime: i32,
    pub iLastTime: i32,
    pub iBestTime: i32,
    pub sessionTimeLeft: f32,
    pub distanceTraveled: f32,
    pub isInPit: i32,
    pub currentSectorIndex: i32,
    pub lastSectorTime: i32,
    pub numberOfLaps: i32,
    pub tyreCompound: [u16; 33],
    pub replayTimeMultiplier: f32,
    pub normalizedCarPosition: f32,
    pub carCoordinates: [f32; 3],
    pub penaltyTime: f32,
}

/// Static page, truncated after `trackSPlineLength`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PageFileStatic {
    pub smVersion: [u16; 15],
    pub acVersion: [u16; 15],
    pub numberOfSessions: i32,
    pub numCars: i32,
    pub carModel: [u16; 33],
    pub track: [u16; 33],
    pub playerName: [u16; 33],
    pub playerSurname: [u16; 33],
    pub playerNick: [u16; 33],
    pub sectorCount: i32,
    pub maxTorque: f32,
    pub maxPower: f32,
    pub maxRpm: i32,
    pub maxFuel: f32,
    pub suspensionMaxTravel: [f32; 4],
    pub tyreRadius: [f32; 4],
    pub maxTurboBoost: f32,
    pub deprecated1: f32,
    pub deprecated2: f32,
    pub penaltiesEnabled: i32,
    pub aidFuelRate: f32,
    pub aidTireRate: f32,
    pub aidMechanicalDamage: f32,
    pub aidAllowTyreBlankets: f32,
    pub aidStability: f32,
    pub aidAutoClutch: i32,
    pub aidAutoBlip: i32,
    pub hasDRS: i32,
    pub hasERS: i32,
    pub hasKERS: i32,
    pub kersMaxJ: f32,
    pub engineBrakeSettingsCount: i32,
    pub ersPowerControllerCount: i32,
    pub trackSPlineLength: f32,
}

impl PageFileStatic {
    pub fn track_name(&self) -> String {
        wide_to_string(&self.track)
    }
    pub fn car_name(&self) -> String {
        wide_to_string(&self.carModel)
    }
}

/// Merge a Physics + Graphics snapshot into a normalized [`TelemetryFrame`].
pub fn merge_frame(t_us: u64, p: &PageFilePhysics, g: &PageFileGraphic) -> TelemetryFrame {
    TelemetryFrame {
        t_us,
        packet_id: p.packetId.max(0) as u32,
        gas: p.gas,
        brake: p.brake,
        gear: p.gear,
        steer_angle: p.steerAngle,
        speed_kmh: p.speedKmh,
        velocity: p.velocity,
        acc_g: p.accG,
        wheel_slip: p.wheelSlip,
        normalized_car_position: g.normalizedCarPosition,
        car_coordinates: g.carCoordinates,
        completed_laps: g.completedLaps,
        i_current_time_ms: g.iCurrentTime,
        i_last_time_ms: g.iLastTime,
        number_of_tyres_out: p.numberOfTyresOut,
        penalty: g.penaltyTime != 0.0,
        is_in_pit: g.isInPit != 0,
        status: AcStatus::from_flag(g.status),
        session: g.session,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::offset_of;

    // Document the layout this code expects (classic AC SDK). If these fail on a
    // given AC/CSP version, the live reader must be re-aligned (spec §4.2).
    #[test]
    fn physics_offsets_match_classic_layout() {
        assert_eq!(offset_of!(PageFilePhysics, packetId), 0);
        assert_eq!(offset_of!(PageFilePhysics, gas), 4);
        assert_eq!(offset_of!(PageFilePhysics, brake), 8);
        assert_eq!(offset_of!(PageFilePhysics, steerAngle), 24);
        assert_eq!(offset_of!(PageFilePhysics, speedKmh), 28);
        assert_eq!(offset_of!(PageFilePhysics, velocity), 32);
        assert_eq!(offset_of!(PageFilePhysics, wheelSlip), 56);
    }

    #[test]
    fn graphics_offsets_match_classic_layout() {
        assert_eq!(offset_of!(PageFileGraphic, packetId), 0);
        assert_eq!(offset_of!(PageFileGraphic, status), 4);
        assert_eq!(offset_of!(PageFileGraphic, completedLaps), 132);
        assert_eq!(offset_of!(PageFileGraphic, iCurrentTime), 140);
        assert_eq!(offset_of!(PageFileGraphic, isInPit), 160);
    }
}
