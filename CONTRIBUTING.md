# Contributing to DrivingCoach

Thanks for your interest! A few ground rules keep this project coherent.

## The spec is the source of truth

[`docs/sim-racing-coaching-system-design-v1.3.md`](docs/sim-racing-coaching-system-design-v1.3.md)
is authoritative for all design decisions, the data model, algorithms, and
acceptance criteria. If a change deviates from the spec, say so explicitly in the
PR and explain why; don't quietly diverge.

## Development

```sh
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

All four must pass. CI runs them on Linux, Windows, and macOS.

### What runs where

The **offline pipeline** crates (`coach-core`, `coach-telemetry`,
`coach-storage`, `coach-analysis`, `coach-cue`, `coach-summary`) are
cross-platform and fully testable without Assetto Corsa, against the synthetic
fixtures / recorded laps. Develop and test these anywhere.

The **real-time tier** (`coach-rt`: live AC shared memory + WASAPI audio) is
gated behind `#[cfg(windows)]` and is exercised on the Windows deploy box. Keep
platform-specific code inside that crate behind `cfg`.

## Guidelines

- **No allocation, locks, or I/O on the real-time tick path.** This is a hard
  invariant (spec §2, §11). Pre-allocate; use the lock-free SPSC ring to hand
  audio to the output thread.
- **No floating-point clock-time comparisons of laps** — always compare by track
  position on the 2 m distance grid (spec §6).
- Prefer **deterministic, testable** logic. Diagnosis and the summary are
  template-generated; there is no model in the loop (spec §1, §10).
- Add a test for every acceptance-criterion behavior you touch (spec §14).
- Never commit voice models (`*.onnx` / `*.onnx.json`) or runtime data
  (`*.db`, recordings, reports). See `.gitignore`.

## Licensing

Code contributions are accepted under the project's [MIT License](LICENSE). Note
the default voice's separate CC BY-NC 4.0 terms — see [`docs/LICENSING.md`](docs/LICENSING.md).
