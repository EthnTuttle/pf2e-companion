//! Phase 12.3 live-lane GM host (docs/phase12/PLAN.md §3.4).
//!
//! A test-only stand-in for the desktop host process: it builds a
//! `SessionService` exactly like `tests/phase10_lan.rs` (pinned
//! `SessionMeta`, `--pin <PIN>`, default A7K9M2), serves the LAN WS bridge
//! on an ephemeral loopback port, prints a single ready line to stdout
//!
//!     {"type":"ready","port":<port>,"session_id":"lan-test","pin":"<pin>"}
//!
//! and then loops over NDJSON on stdin: each line `{"intent": <Intent>}` is
//! applied to the service as the GM (`apply_tracked` — the same entry point
//! the IPC layer uses). Every resulting PatchEvent is streamed to stdout as
//! `{"type":"patch","event":{…}}` (one line each), followed by exactly one
//! ack line: `{"type":"applied","ok":true}` or
//! `{"type":"applied","ok":false,"error":"…"}` — so the Playwright side
//! (tests/e2e/server/liveHost.ts) can synchronize on effects instead of
//! sleeping.
//!
//! The adventure resolver serves the bundled-TEST fixture
//! (tests/fixtures/adventure/widows-lamp.md, parsed on boot) so L4's
//! `load_adventure("widows-lamp")` is deterministic and independent of the
//! promoted content tree.
//!
//! Pinned identity values mirror tests/e2e/fixtures/session.ts by contract
//! (the two sides never import each other): session id "lan-test", GM peer
//! "gm-peer" / display name "GM Dana". The GM is registered in the roster at
//! boot (`peer_joined`, like service.rs host() does) so the party list on a
//! joined client mirrors a real session.
//!
//! Never run under `cargo test`: Cargo only compiles examples there.

#![cfg(feature = "lan")]

use include_dir::{Dir, include_dir};
use pf2e_companion_lib::adventure;
use pf2e_companion_lib::lan;
use pf2e_companion_lib::session::doc::{SessionMeta, SharedDoc};
use pf2e_companion_lib::session::service::{self, SessionService};
use pf2e_companion_lib::session::{Intent, Role, SessionDoc};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::Write;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};

static ADVENTURE_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/tests/fixtures/adventure");

const SESSION_ID: &str = "lan-test";
const SESSION_NAME: &str = "LAN Table";
const GM_PEER: &str = "gm-peer";
const GM_NAME: &str = "GM Dana";
const DEFAULT_PIN: &str = "A7K9M2";

/// One stdin command line: `{"intent": <Intent>}`.
#[derive(Debug, Deserialize)]
struct Command {
    intent: Intent,
}

/// `--pin <PIN>` (default A7K9M2). Anything else is ignored — this binary is
/// driven by the test harness, not by hand.
fn parse_pin() -> String {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--pin" {
            if let Some(pin) = args.next() {
                return pin;
            }
        }
    }
    DEFAULT_PIN.to_string()
}

/// Same frontmatter split as tests/phase10_adventure.rs (the fixture pack
/// format the content loader produces).
fn split_frontmatter(s: &str) -> (&str, &str) {
    let trimmed = s.trim_start_matches('\u{FEFF}');
    if !trimmed.starts_with("---") {
        return ("", trimmed);
    }
    let after = trimmed[3..].trim_start_matches(['\r', '\n']);
    match after.find("\n---") {
        Some(end) => (&after[..end], &after[end + 4..]),
        None => ("", trimmed),
    }
}

/// A one-slug resolver over the bundled-test adventure fixture.
fn fixture_adventures() -> service::AdventureResolver {
    let raw = ADVENTURE_DIR
        .get_file("widows-lamp.md")
        .expect("fixture adventure present")
        .contents_utf8()
        .expect("fixture adventure is utf8");
    let (fm, body) = split_frontmatter(raw);
    let value: serde_yaml::Value = serde_yaml::from_str(fm).expect("fixture frontmatter is yaml");
    let serde_yaml::Value::Mapping(mapping) = value else {
        panic!("fixture frontmatter not a mapping")
    };
    let def = adventure::parse_adventure(&mapping, body, &ADVENTURE_DIR, "widows-lamp")
        .expect("fixture adventure parses");
    Arc::new(move |slug: &str| {
        if slug == "widows-lamp" {
            Some(def.clone())
        } else {
            None
        }
    })
}

/// One NDJSON line to stdout, flushed (the harness parses by line).
fn out_line(v: &Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let pin = parse_pin();
    let meta = SessionMeta {
        id: SESSION_ID.to_string(),
        name: SESSION_NAME.to_string(),
        gm_id: GM_PEER.to_string(),
        created_at: 1_700_000_000_000,
        adventure_slug: None,
        ended: false,
    };
    let svc = SessionService::new(
        SharedDoc::new(SessionDoc::new_session(meta)),
        None,
        fixture_adventures(),
        Some(pin.clone()),
    );
    // Register the GM in the roster, mirroring service.rs host().
    svc.peer_joined(GM_PEER, GM_NAME, "ipc")
        .expect("gm joins own session");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback");
    let port = listener.local_addr().expect("local addr").port();
    let bridge = svc.clone();
    tokio::spawn(async move {
        if let Err(e) = lan::serve(listener, bridge).await {
            eprintln!("lan serve ended: {e}");
        }
    });

    out_line(&json!({
        "type": "ready",
        "port": port,
        "session_id": SESSION_ID,
        "name": SESSION_NAME,
        "pin": pin,
    }));

    // NDJSON intent loop. Stdin EOF (harness close) exits the process,
    // which drops the listener and frees the port.
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    loop {
        let line = match lines.next_line().await {
            Ok(Some(line)) => line,
            Ok(None) | Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<Command>(trimmed) {
            Ok(cmd) => match svc.apply_tracked(GM_PEER, Role::Gm, cmd.intent).await {
                Ok((events, _roll)) => {
                    for event in events {
                        out_line(&json!({ "type": "patch", "event": event }));
                    }
                    out_line(&json!({ "type": "applied", "ok": true }));
                }
                Err(e) => {
                    out_line(&json!({ "type": "applied", "ok": false, "error": e.to_string() }));
                }
            },
            Err(e) => {
                out_line(&json!({
                    "type": "applied",
                    "ok": false,
                    "error": format!("bad command: {e}"),
                }));
            }
        }
    }
}
