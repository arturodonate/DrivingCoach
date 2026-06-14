# DrivingCoach

An **audio-only AI driving coach** for offline hotlapping in
[Assetto Corsa](https://www.assettocorsa.it/) (with CSP / Content Manager) on
Windows 11. The driver hears short spoken cues before each coached corner —
*"Turn 5 — brake slightly later"*, *"apex"*, *"better — Turn 3"* — derived from a
**deterministic** analysis of their previous lap, like a coach in the passenger
seat. The goal is measurably lower lap times.

No HUD, no overlays, no LLM, no GPU use — everything is CPU + RAM. See the full
design in [`docs/sim-racing-coaching-system-design-v1.3.md`](docs/sim-racing-coaching-system-design-v1.3.md),
which is the authoritative specification this project implements.

> **Status:** under active construction. The **offline analysis pipeline**
> (telemetry recording → resampling → reference model → diagnosis → cue
> compilation → post-event summary) is being built and tested first, against
> recorded laps, exactly as the spec's build order (§13) intends. The
> **real-time tier** (live AC shared-memory ingestion + WASAPI audio) is
> Windows-only and lands in a follow-up increment on the deploy hardware.

## How it works

The system splits *what to say* from *when to say it* (spec §2):

- **Between laps** (worker thread, milliseconds of arithmetic): analyze the lap
  you just drove, diagnose where and *why* time was lost, and compile a coaching
  plan of pre-synthesized audio cues for the next lap.
- **In real time** (60 Hz tick, allocation-free / lock-free): watch car position
  and fire the pre-rendered clips at speed-compensated trigger points so each cue
  *finishes* just before its corner.

Nothing on the real-time path synthesizes speech, allocates, touches disk, or
waits on a lock.

## Architecture

A single Rust process, organized as a Cargo workspace:

| Crate | Role |
|---|---|
| [`coach-core`](crates/coach-core) | Domain types, units, distance grid, config, diagnosis & cue objects. Pure, no I/O. |
| [`coach-telemetry`](crates/coach-telemetry) | AC shared-memory `#[repr(C)]` structs; telemetry sources (live Windows / recorded replay); integrity checks; recorder. |
| [`coach-storage`](crates/coach-storage) | Embedded SQLite store (`rusqlite`) behind a `Store` trait. |
| [`coach-analysis`](crates/coach-analysis) | Resampler, Savitzky–Golay profiler, reference builder, alignment + diagnosis engine. |
| [`coach-cue`](crates/coach-cue) | Closed cue-template grammar, cue compiler, trigger back-integration, arbiter, Piper TTS + WAV cache. |
| [`coach-summary`](crates/coach-summary) | Deterministic post-event performance summary (`.md` + `.json`). |
| [`coach-rt`](crates/coach-rt) | **Windows-only**, scaffolded: 60 Hz cue engine + WASAPI audio thread. |
| [`coach`](crates/coach) | CLI binary that orchestrates the pipeline. |

## Building

Requires a stable Rust toolchain ([rustup](https://rustup.rs)).

```sh
# Develop and test the offline pipeline anywhere (macOS / Linux / Windows):
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

### Windows deploy build (the target machine)

Build natively on the Windows 11 / Ryzen 7800X3D box — it has the MSVC linker and
Windows SDK that the real-time tier (`memmap2` shared memory + `cpal`/WASAPI audio)
needs:

```powershell
cargo build --release
```

**Confirm AC field layout first.** The `#[repr(C)]` shared-memory structs in
`coach-telemetry` mirror the documented AC SDK pages, but exact field
names/offsets **must be verified against your installed AC/CSP version** (spec
§4.2) — CSP can extend these pages. The struct module flags this inline.

**One free win** (spec §11): cap AC at ~144–150 fps to match the display; frames
above refresh become CPU headroom and frametime stability for the coach.

## Voice / TTS setup

Cues are pre-synthesized with [Piper](https://github.com/rhasspy/piper) (CPU,
offline) using the **`en_US-ryan-medium`** voice. The voice model is **not
committed** — fetch it at install time:

```sh
./scripts/setup-voice.sh        # macOS / Linux
.\scripts\setup-voice.ps1       # Windows (PowerShell)
```

This downloads `en_US-ryan-medium.onnx` + `.onnx.json` from the official
`rhasspy/piper-voices` repository into `voices/`. You also need the `piper`
binary and `espeak-ng` on `PATH` (the script prints install hints).

## ⚠️ Licensing — read before redistributing

- **Project code:** [MIT](LICENSE).
- **Bundled-by-download voice (`en_US-ryan-medium`):** trained on the
  **RyanSpeech** corpus and licensed **CC BY-NC 4.0 — non-commercial,
  attribution required** (Zandie et al., 2021). Because the voice inherits the
  dataset license, **this project is for non-commercial use as configured.**
- The voice is **swappable via config**: a LibriTTS/LibriTTS-R-derived Piper
  voice (CC BY 4.0) permits commercial use. See [`docs/LICENSING.md`](docs/LICENSING.md)
  for the full breakdown, attribution text, and how to swap.

## License

Code is MIT (see [`LICENSE`](LICENSE)). The downloaded voice model is licensed
separately; see [`docs/LICENSING.md`](docs/LICENSING.md).
