# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Cargo workspace scaffold and repo hygiene (CI, license docs, voice-setup scripts).
- Implementing the offline analysis pipeline per spec §13 steps 1–5 and 7:
  telemetry recorder, resampler/profiler, reference builder, diagnosis engine,
  cue compiler + Piper cache, and the post-event performance summary.
- `coach-rt` crate scaffolded for the Windows-only real-time tier (step 6).
