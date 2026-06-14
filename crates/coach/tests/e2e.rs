//! End-to-end CLI test: `gen-fixture` then `analyze` must run the whole offline
//! pipeline and write a summary, with no Assetto Corsa and no Piper.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_coach")
}

#[test]
fn gen_fixture_then_analyze_writes_a_summary() {
    let tmp = std::env::temp_dir().join(format!("coach_e2e_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let recording = tmp.join("session.bincode");
    let db = tmp.join("coach.db");
    let reports = tmp.join("reports");

    let gen = Command::new(bin())
        .args(["gen-fixture"])
        .arg(&recording)
        .args(["--laps", "8", "--track", "test_track", "--car", "test_car"])
        .status()
        .expect("run gen-fixture");
    assert!(gen.success());
    assert!(recording.exists());

    let analyze = Command::new(bin())
        .args(["analyze"])
        .arg(&recording)
        .arg("--db")
        .arg(&db)
        .arg("--reports")
        .arg(&reports)
        .output()
        .expect("run analyze");
    assert!(
        analyze.status.success(),
        "analyze failed: {}",
        String::from_utf8_lossy(&analyze.stderr)
    );
    assert!(db.exists(), "database created");

    // Exactly one .md and one .json summary written.
    let entries: Vec<PathBuf> = std::fs::read_dir(&reports)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    let md = entries
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .count();
    let json = entries
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .count();
    assert_eq!(md, 1, "one markdown summary");
    assert_eq!(json, 1, "one json summary");

    // The markdown carries the PACE section and a clean-lap count.
    let md_path = entries
        .iter()
        .find(|p| p.extension().is_some_and(|e| e == "md"))
        .unwrap();
    let body = std::fs::read_to_string(md_path).unwrap();
    assert!(body.contains("PACE"));
    assert!(body.contains("clean laps"));

    let _ = std::fs::remove_dir_all(&tmp);
}
