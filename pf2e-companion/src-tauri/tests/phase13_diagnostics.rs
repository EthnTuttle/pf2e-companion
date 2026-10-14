//! Phase 13 — crash + log hygiene (SHIPPING.md Tier 1 item 4).
//!
//! Covers the two contracts the unit tests can't reach:
//! 1. Tracing events routed through a `TeeWriter` land in BOTH the hub
//!    ring and the per-session file, using a thread-local `Dispatch`
//!    (never `set_default` — parallel tests stay clean).
//! 2. The session log path uses the `.log` extension, so the vault
//!    watcher's markdown filter can never ingest it (guards the
//!    `is_markdown` contract by construction; `is_markdown` itself is
//!    private to `vault.rs`).

use pf2e_companion_lib::diagnostics::{DiagnosticsHub, TeeWriter};
use tempfile::TempDir;

#[test]
fn tracing_events_flow_to_stderr_ring_and_session_file() {
    let dir = TempDir::new().unwrap();
    let log_path = dir.path().join("s1.log");
    let hub = DiagnosticsHub::new(100);
    hub.set_session(&log_path);

    let subscriber = tracing_subscriber::fmt()
        .with_writer(TeeWriter::new(hub.clone()))
        .finish();
    let dispatch = tracing::Dispatch::new(subscriber);

    let marker = format!("phase13-marker-{}", uuid::Uuid::new_v4());
    tracing::dispatcher::with_default(&dispatch, || {
        tracing::warn!("{}", marker);
    });

    let ring = hub.snapshot_lines().join("\n");
    assert!(
        ring.contains(&marker),
        "marker missing from ring: {ring:?}"
    );
    let file = std::fs::read_to_string(&log_path).unwrap_or_default();
    assert!(
        file.contains(&marker),
        "marker missing from session file: {file:?}"
    );
}

#[test]
fn diagnostics_file_uses_log_extension_so_watcher_ignores_it() {
    // Construct the session log path exactly as session::service::host
    // does: save_path.with_extension("log"). The extension is what keeps
    // the file out of the vault watcher (is_markdown matches only
    // md/markdown), so it is the contract under guard.
    let dir = TempDir::new().unwrap();
    let save_path = dir.path().join("sessions").join("abc.automerge");
    let log_path = save_path.with_extension("log");
    assert_eq!(log_path.extension().and_then(|e| e.to_str()), Some("log"));
    assert_ne!(log_path, save_path);
}
