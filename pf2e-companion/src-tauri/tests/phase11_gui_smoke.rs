//! Phase 11 — GUI smoke runner (CONTRACT-11 § 1).
//!
//! Delegates to `tests/gui_smoke.sh`, which self-gates on
//! `PF2E_GUI_SMOKE=1` (and skips cleanly when xvfb-run, curl, or pnpm
//! are missing). The default `cargo test` run therefore never touches
//! a display or spawns dev servers.

#[test]
fn gui_smoke_launches_cleanly_under_xvfb() {
    if std::env::var("PF2E_GUI_SMOKE").as_deref() != Ok("1") {
        eprintln!("SKIP: PF2E_GUI_SMOKE not set (gui smoke is opt-in)");
        return;
    }
    let status = std::process::Command::new("bash")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/gui_smoke.sh"))
        .status()
        .expect("spawn tests/gui_smoke.sh");
    assert!(status.success(), "gui smoke failed with {status}");
}
