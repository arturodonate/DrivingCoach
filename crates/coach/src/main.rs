//! DrivingCoach CLI — orchestrates the offline pipeline (and, on Windows, the
//! live coach). The offline subcommands run anywhere; `run` is the Windows-only
//! real-time tier (a follow-up increment).

mod fixture;
mod pipeline;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use coach_core::config::Config;
use coach_telemetry::Recording;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "coach",
    version,
    about = "Audio-only AI driving coach for Assetto Corsa"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a synthetic recorded session (no Assetto Corsa needed).
    GenFixture {
        /// Output recording file (.bincode).
        out: PathBuf,
        #[arg(long, default_value_t = 8)]
        laps: usize,
        #[arg(long, default_value = "stadium_demo")]
        track: String,
        #[arg(long, default_value = "demo_car")]
        car: String,
    },
    /// Run the full offline pipeline on a recording: profile → reference →
    /// diagnose → store → write the post-event summary.
    Analyze {
        /// Input recording file (.bincode), e.g. from `gen-fixture`.
        recording: PathBuf,
        /// SQLite database file.
        #[arg(long, default_value = "data/coach.db")]
        db: PathBuf,
        /// Directory for the `.md` / `.json` summary artifacts.
        #[arg(long, default_value = "reports")]
        reports: PathBuf,
    },
    /// Live coaching (Windows only) — the real-time tier (follow-up increment).
    Run,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::GenFixture {
            out,
            laps,
            track,
            car,
        } => {
            let rec = fixture::generate(&track, &car, laps);
            let frames = rec.frames.len();
            rec.write_to(&out).context("writing recording")?;
            println!(
                "Wrote {frames} frames ({laps} laps) for {track} / {car} → {}",
                out.display()
            );
        }
        Command::Analyze {
            recording,
            db,
            reports,
        } => {
            let rec = Recording::read_from(&recording).context("reading recording")?;
            let cfg = Config::default();
            let outcome = pipeline::analyze(rec, &db, &reports, &cfg)?;
            println!(
                "Session {} — {} clean laps",
                outcome.session_id, outcome.clean_laps
            );
            println!("  summary: {}", outcome.md_path.display());
            println!("  json:    {}", outcome.json_path.display());
        }
        Command::Run => {
            eprintln!(
                "Live coaching (the real-time tier) ships in the Windows increment.\n\
                 The offline pipeline is available now via `coach analyze`."
            );
        }
    }
    Ok(())
}
