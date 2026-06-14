//! Live Assetto Corsa telemetry source (Windows-only).
//!
//! AC exposes three named, page-file-backed shared-memory mappings —
//! `Local\acpmf_physics`, `Local\acpmf_graphics`, `Local\acpmf_static` — created
//! by the game. We open them read-only with Win32 `OpenFileMappingW` +
//! `MapViewOfFile` and read the `#[repr(C)]` pages from [`crate::ac_shared`].
//!
//! Physics is polled and **deduplicated on `packetId`** (spec §4.1): a poll that
//! sees an unchanged id returns `None`. The caller drives the 100 Hz / 60 Hz
//! cadence.
//!
//! ⚠️ The page layouts must be confirmed against the installed AC/CSP version
//! (spec §4.2). This module is untested off-Windows; CI compiles it on the
//! Windows runner.
#![cfg(windows)]

use crate::ac_shared::{merge_frame, PageFileGraphic, PageFilePhysics, PageFileStatic};
use crate::error::TelemetryError;
use crate::frame::TelemetryFrame;
use crate::source::{StaticInfo, TelemetrySource};
use std::ffi::c_void;
use std::ptr;
use std::time::Instant;

type Handle = *mut c_void;

const FILE_MAP_READ: u32 = 0x0004;

extern "system" {
    fn OpenFileMappingW(dwDesiredAccess: u32, bInheritHandle: i32, lpName: *const u16) -> Handle;
    fn MapViewOfFile(
        hFileMappingObject: Handle,
        dwDesiredAccess: u32,
        dwFileOffsetHigh: u32,
        dwFileOffsetLow: u32,
        dwNumberOfBytesToMap: usize,
    ) -> *mut c_void;
    fn UnmapViewOfFile(lpBaseAddress: *const c_void) -> i32;
    fn CloseHandle(hObject: Handle) -> i32;
}

fn wide(name: &str) -> Vec<u16> {
    name.encode_utf16().chain(std::iter::once(0)).collect()
}

/// One mapped, read-only page.
struct MappedPage {
    handle: Handle,
    view: *const c_void,
}

impl MappedPage {
    fn open(name: &str, size: usize) -> Result<Self, TelemetryError> {
        // SAFETY: FFI to the documented Win32 API with a NUL-terminated name.
        let handle = unsafe { OpenFileMappingW(FILE_MAP_READ, 0, wide(name).as_ptr()) };
        if handle.is_null() {
            return Err(TelemetryError::SharedMemory(format!(
                "OpenFileMappingW failed for {name} — is Assetto Corsa running?"
            )));
        }
        // SAFETY: handle is valid; mapping the whole page read-only.
        let view = unsafe { MapViewOfFile(handle, FILE_MAP_READ, 0, 0, size) };
        if view.is_null() {
            // SAFETY: handle is a valid mapping object.
            unsafe { CloseHandle(handle) };
            return Err(TelemetryError::SharedMemory(format!(
                "MapViewOfFile failed for {name}"
            )));
        }
        Ok(Self {
            handle,
            view: view as *const c_void,
        })
    }

    /// Read a `Copy` page out of the mapping. Unaligned-safe.
    ///
    /// SAFETY: `T` must be `#[repr(C)]` matching the live page and no larger than
    /// the mapped region.
    unsafe fn read<T: Copy>(&self) -> T {
        unsafe { ptr::read_unaligned(self.view as *const T) }
    }
}

impl Drop for MappedPage {
    fn drop(&mut self) {
        // SAFETY: both came from successful Win32 calls and are dropped once.
        unsafe {
            UnmapViewOfFile(self.view);
            CloseHandle(self.handle);
        }
    }
}

/// Live source over AC shared memory.
pub struct LiveAcSource {
    physics: MappedPage,
    graphics: MappedPage,
    static_info: StaticInfo,
    last_packet_id: i32,
    start: Instant,
}

impl LiveAcSource {
    /// Open all three pages. Fails if AC is not running.
    pub fn connect() -> Result<Self, TelemetryError> {
        let physics = MappedPage::open(
            "Local\\acpmf_physics",
            std::mem::size_of::<PageFilePhysics>(),
        )?;
        let graphics = MappedPage::open(
            "Local\\acpmf_graphics",
            std::mem::size_of::<PageFileGraphic>(),
        )?;
        let static_page =
            MappedPage::open("Local\\acpmf_static", std::mem::size_of::<PageFileStatic>())?;

        // SAFETY: page opened to at least size_of::<PageFileStatic>().
        let s: PageFileStatic = unsafe { static_page.read() };
        let static_info = StaticInfo {
            track: s.track_name(),
            car_model: s.car_name(),
            track_spline_length_m: s.trackSPlineLength as f64,
        };

        Ok(Self {
            physics,
            graphics,
            static_info,
            last_packet_id: -1,
            start: Instant::now(),
        })
    }
}

impl TelemetrySource for LiveAcSource {
    fn poll(&mut self) -> Option<TelemetryFrame> {
        // SAFETY: pages opened to at least the struct size.
        let p: PageFilePhysics = unsafe { self.physics.read() };
        if p.packetId == self.last_packet_id {
            return None; // dedupe on packetId (spec §4.1)
        }
        self.last_packet_id = p.packetId;
        let g: PageFileGraphic = unsafe { self.graphics.read() };
        let t_us = self.start.elapsed().as_micros() as u64;
        Some(merge_frame(t_us, &p, &g))
    }

    fn static_info(&self) -> Option<&StaticInfo> {
        Some(&self.static_info)
    }
}
