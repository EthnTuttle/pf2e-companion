//! Crash + log hygiene (SHIPPING.md Tier 1 item 4).
//!
//! Two sinks, one hub:
//! - a bounded in-memory ring (the last 2,000 lines, ~400 KB max) that
//!   `diagnostics_get` snapshots into a paste-ready blob, and
//! - a per-session file `<vault>/campaigns/<id>/sessions/<session-id>.log`
//!   (append mode, one `=== session log started … ===` marker per run) that
//!   only the **host** process writes — peers (`join_peer`, no save path)
//!   log to the ring only. The GM's host process is the one that files bug
//!   reports.
//!
//! The `.log` extension keeps the file out of the vault watcher by
//! construction (`is_markdown` matches only `md`/`markdown`; Phase 12's
//! `is_session_artifact` additionally skips `sessions/` artifacts).
//!
//! No telemetry: nothing here ever leaves the machine unless the user
//! pastes the blob into a GitHub issue. The ring is the only bounded
//! memory; a long-lived resumed session's `.log` grows across resumes —
//! acceptable at info-level volume for beta, revisit if it ever matters.
//!
//! Diagnostics must never take the app down: all session-file IO errors
//! are swallowed.

use std::collections::VecDeque;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tracing_subscriber::fmt::MakeWriter;

/// Ring capacity for the app hub (plan § 2 step 1: 2,000 lines).
const APP_RING_CAP: usize = 2_000;

/// Paste-ready diagnostics blob (the `diagnostics_get` IPC payload).
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticsInfo {
    pub app_version: &'static str,
    pub os: &'static str,
    pub arch: &'static str,
    pub session_id: Option<String>,
    pub lines: Vec<String>,
}

struct SessionSink {
    path: PathBuf,
}

/// Bounded in-memory ring + optional per-session file sink.
pub struct DiagnosticsHub {
    ring: Mutex<VecDeque<String>>,
    cap: usize,
    session: Mutex<Option<SessionSink>>,
}

impl DiagnosticsHub {
    /// A standalone hub (tests build their own; the app uses [`hub`]).
    pub fn new(cap: usize) -> Arc<Self> {
        Arc::new(DiagnosticsHub {
            ring: Mutex::new(VecDeque::new()),
            cap,
            session: Mutex::new(None),
        })
    }

    /// Append one complete log line: to the ring (evicting the oldest
    /// past cap) and, when a session sink is set, to the session file.
    /// File IO errors are swallowed — diagnostics never crash the app.
    pub fn push_line(&self, line: &str) {
        {
            let mut ring = self.ring.lock().expect("diagnostics ring lock");
            if ring.len() >= self.cap {
                ring.pop_front();
            }
            ring.push_back(line.to_string());
        }
        let path = {
            let session = self.session.lock().expect("diagnostics session lock");
            session.as_ref().map(|s| s.path.clone())
        };
        if let Some(path) = path {
            use std::io::Write;
            let append = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .and_then(|mut f| writeln!(f, "{line}"));
            if let Err(e) = append {
                // Swallowed on purpose; keep the ring rolling regardless.
                let _ = e;
            }
        }
    }

    /// Point the session sink at `path` (append mode — a resumed session
    /// keeps earlier history below a fresh marker line).
    pub fn set_session(&self, path: &Path) {
        {
            let mut session = self.session.lock().expect("diagnostics session lock");
            *session = Some(SessionSink {
                path: path.to_path_buf(),
            });
        }
        let marker = format!(
            "=== session log started {} ===",
            chrono::Utc::now().to_rfc3339()
        );
        self.push_line(&marker);
    }

    /// Drop the session sink (the ring keeps rolling).
    pub fn clear_session(&self) {
        self.session
            .lock()
            .expect("diagnostics session lock")
            .take();
    }

    /// Ring copy (oldest first).
    pub fn snapshot_lines(&self) -> Vec<String> {
        self.ring
            .lock()
            .expect("diagnostics ring lock")
            .iter()
            .cloned()
            .collect()
    }

    /// The paste-ready blob for `diagnostics_get`.
    pub fn blob(&self, active_session_id: Option<&str>) -> DiagnosticsInfo {
        DiagnosticsInfo {
            app_version: env!("CARGO_PKG_VERSION"),
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            session_id: active_session_id.map(|s| s.to_string()),
            lines: self.snapshot_lines(),
        }
    }
}

static HUB: OnceLock<Arc<DiagnosticsHub>> = OnceLock::new();

/// The app's hub (cap 2,000 lines). Tests build their own via
/// [`DiagnosticsHub::new`] so they share no state with this one.
pub fn hub() -> &'static Arc<DiagnosticsHub> {
    HUB.get_or_init(|| DiagnosticsHub::new(APP_RING_CAP))
}

/// `MakeWriter` that tees the fmt layer's output: every raw byte goes to
/// `stderr` (byte-identical to the pre-diagnostics console), and bytes are
/// buffered until `\n` before being pushed into the hub as complete lines
/// (the fmt layer may deliver one line in several `write` calls).
pub struct TeeWriter {
    hub: Arc<DiagnosticsHub>,
}

impl TeeWriter {
    pub fn new(hub: Arc<DiagnosticsHub>) -> Self {
        TeeWriter { hub }
    }
}

impl<'a> MakeWriter<'a> for TeeWriter {
    type Writer = TeeHandle;

    fn make_writer(&'a self) -> Self::Writer {
        TeeHandle {
            hub: self.hub.clone(),
            buf: Vec::new(),
        }
    }
}

/// One handle per fmt-layer write session; buffers partial lines until
/// the next `\n`.
pub struct TeeHandle {
    hub: Arc<DiagnosticsHub>,
    buf: Vec<u8>,
}

impl io::Write for TeeHandle {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Mirror the raw bytes first so stderr stays byte-identical.
        std::io::stderr().write_all(buf)?;
        self.buf.extend_from_slice(buf);
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = self.buf.drain(..=pos).collect();
            line.pop(); // the '\n'
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches('\r');
            self.hub.push_line(line);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        // No-op beyond stderr (partial lines stay buffered until '\n';
        // the global subscriber handles flush ordering).
        std::io::stderr().flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn sink_path(dir: &TempDir) -> PathBuf {
        dir.path().join("s1.log")
    }

    #[test]
    fn ring_evicts_oldest_past_cap() {
        let hub = DiagnosticsHub::new(3);
        for i in 1..=5 {
            hub.push_line(&format!("line-{i}"));
        }
        assert_eq!(
            hub.snapshot_lines(),
            vec!["line-3", "line-4", "line-5"]
        );
    }

    #[test]
    fn fragmented_writes_assemble_into_lines() {
        let hub = DiagnosticsHub::new(100);
        let tee = TeeWriter::new(hub.clone());
        let mut w = tee.make_writer();
        w.write_all(b"par").unwrap();
        w.write_all(b"tial\nsec").unwrap();
        w.write_all(b"ond\n").unwrap();
        assert_eq!(hub.snapshot_lines(), vec!["partial", "second"]);
    }

    #[test]
    fn session_sink_appends_and_marker_written() {
        let dir = TempDir::new().unwrap();
        let hub = DiagnosticsHub::new(100);
        hub.set_session(&sink_path(&dir));
        hub.push_line("hello");
        hub.clear_session();
        hub.push_line("ring-only");

        let contents = std::fs::read_to_string(sink_path(&dir)).unwrap();
        assert!(contents.contains("=== session log started "));
        assert!(contents.contains(" ===\n"));
        assert!(contents.contains("hello\n"));
        assert!(!contents.contains("ring-only"));
        // Marker + both lines in the ring; the ring keeps rolling after
        // clear_session.
        let ring = hub.snapshot_lines();
        assert_eq!(ring.len(), 3);
        assert!(ring[0].starts_with("=== session log started "));
        assert_eq!(ring[1], "hello");
        assert_eq!(ring[2], "ring-only");
    }

    #[test]
    fn blob_contains_app_metadata() {
        let hub = DiagnosticsHub::new(100);
        hub.push_line("one");
        let blob = hub.blob(Some("s1"));
        assert_eq!(blob.app_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(blob.os, std::env::consts::OS);
        assert_eq!(blob.arch, std::env::consts::ARCH);
        assert_eq!(blob.session_id.as_deref(), Some("s1"));
        assert_eq!(blob.lines, vec!["one"]);
    }

    #[test]
    fn session_sink_survives_unwritable_file() {
        let hub = DiagnosticsHub::new(100);
        // A path under a non-directory can never be opened for append.
        let bad = TempDir::new().unwrap();
        let bad_file = bad.path().join("not-a-dir");
        std::fs::write(&bad_file, b"x").unwrap();
        hub.set_session(&bad_file.join("child").with_extension("log"));
        hub.push_line("still-alive"); // must not panic
        assert_eq!(hub.snapshot_lines().last().unwrap(), "still-alive");
    }

    #[test]
    fn peer_mode_writes_ring_only_and_no_file() {
        // Locks in the host-only-files decision (SHIPPING review): a peer
        // never has a sink — the only `set_session` call sites are
        // `service::host` and `service::load`; `join_peer` passes
        // `save_path = None`. With no sink set, lines stay in the ring
        // and nothing touches disk.
        let dir = TempDir::new().unwrap();
        let hub = DiagnosticsHub::new(100);
        hub.push_line("peer-noise");
        assert_eq!(hub.snapshot_lines(), vec!["peer-noise"]);
        assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
    }
}
