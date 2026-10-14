//! Phase 10 — PLAN gate 4 end-to-end table demo (CONTRACT § 6).
//!
//! Host service + one WS client joined via the advertised LAN URL:
//! `roll_dice` (through the session-scoped LLM tool path) is visible on
//! the host emit bus AND as a client patch; a client `update_sheet`
//! targeting the GM's pc is rejected with an `error` frame.

#![cfg(feature = "lan")]

use futures_util::{SinkExt, StreamExt};
use pf2e_companion_lib::db::Db;
use pf2e_companion_lib::llm::ToolCall;
use pf2e_companion_lib::llm_tools;
use pf2e_companion_lib::session::service::{host, no_adventures};
use pf2e_companion_lib::session::{
    CharacterSheet, Intent, PatchEventPayload, Role, SessionHandle,
};
use pf2e_companion_lib::session::doc::SessionEvent;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type ClientWs = WebSocketStream<MaybeTlsStream<TcpStream>>;

fn sheet() -> CharacterSheet {
    CharacterSheet {
        level: 1,
        class: "Fighter".to_string(),
        hp: 18,
        max_hp: 18,
        ac: 18,
        conditions: vec![],
        notes: String::new(),
    }
}

async fn send_json(ws: &mut ClientWs, v: Value) {
    ws.send(Message::Text(v.to_string().into()))
        .await
        .expect("client send");
}

async fn await_frame(ws: &mut ClientWs, want_type: &str, pred: impl Fn(&Value) -> bool) -> Value {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let msg = tokio::time::timeout_at(deadline, ws.next())
            .await
            .expect("timeout waiting for ws frame")
            .expect("ws stream ended")
            .expect("ws read error");
        if let Message::Text(t) = msg {
            let v: Value = serde_json::from_str(t.as_str()).expect("host frame is json");
            if v["type"] == want_type && pred(&v) {
                return v;
            }
        }
    }
}

/// Join a hosted table over its advertised LAN URL; returns the socket
/// post-handshake plus the assigned peer id.
async fn join(url: &str, session_id: &str, name: &str) -> (ClientWs, String) {
    let (mut ws, _) = tokio_tungstenite::connect_async(url.to_string())
        .await
        .expect("connect to advertised lan url");
    send_json(&mut ws, json!({ "type": "hello", "session_id": session_id, "name": name })).await;
    let welcome = await_frame(&mut ws, "welcome", |_| true).await;
    assert_eq!(welcome["session_id"], session_id);
    assert_eq!(welcome["role"], "player");
    (ws, welcome["peer_id"].as_str().unwrap().to_string())
}

#[tokio::test(flavor = "multi_thread")]
async fn roll_dice_reaches_host_bus_and_ws_client() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(&tmp.path().join("table-demo.db")).unwrap();
    let registry = pf2e_companion_lib::llm::LlmRegistry::new();

    let handle: Arc<SessionHandle> = Arc::new(
        host(tmp.path(), "demo-campaign", "Demo GM", None, no_adventures())
            .await
            .expect("host the table"),
    );
    assert_eq!(handle.role, Role::Gm);

    // Host emit bus (what the Tauri event forwarder consumes).
    let mut rx = handle.service().subscribe();

    // A thin client joins via the advertised URL.
    let url = handle.lan_url.clone().expect("host advertises a lan url");
    let (mut ws, _peer_id) = join(&url, &handle.session_id, "Player One").await;

    // The GM rolls through the session-scoped LLM tool path.
    let call = ToolCall {
        id: "tool-1".to_string(),
        name: "roll_dice".to_string(),
        input: json!({ "expr": "1d20+5" }),
    };
    let result =
        llm_tools::dispatch_tool_session(&call, &db, &registry, Some(&handle)).await.expect(
            "roll_dice dispatch succeeds for the GM on the host",
        );
    assert_eq!(result["expr"], "1d20+5");
    let total = result["total"].as_i64().expect("total is a number");
    assert!((6..=25).contains(&total), "1d20+5 total out of range: {total}");

    // Visible on the host emit bus as a chat_appended patch with a roll.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let mut saw_roll_on_bus = false;
    while !saw_roll_on_bus {
        let event = tokio::time::timeout_at(deadline, rx.recv())
            .await
            .expect("timeout waiting for host bus event")
            .expect("host bus closed");
        if let SessionEvent::Patch(p) = event {
            if let PatchEventPayload::ChatAppended { entry } = p.payload {
                if let Some(roll) = entry.roll {
                    assert_eq!(roll.expr, "1d20+5");
                    assert_eq!(roll.total, total);
                    saw_roll_on_bus = true;
                }
            }
        }
    }

    // Visible to the WS client as a chat_appended patch with a roll.
    let frame = await_frame(&mut ws, "patch", |v| {
        v["kind"] == "chat_appended" && v["entry"]["roll"].is_object()
    })
    .await;
    assert_eq!(frame["entry"]["kind"], "roll");
    assert_eq!(frame["entry"]["roll"]["total"], total);

    // And it landed in the canonical doc.
    let snapshot = handle.snapshot_now().unwrap();
    assert_eq!(snapshot.chat.len(), 1);
    let roll = snapshot.chat[0].roll.as_ref().expect("chat entry carries the roll");
    assert_eq!(roll.total, total);
}

#[tokio::test(flavor = "multi_thread")]
async fn secret_roll_leaks_no_numbers_to_clients() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(&tmp.path().join("secret-demo.db")).unwrap();
    let registry = pf2e_companion_lib::llm::LlmRegistry::new();

    let handle: Arc<SessionHandle> = Arc::new(
        host(tmp.path(), "secret-campaign", "Demo GM", None, no_adventures())
            .await
            .expect("host the table"),
    );

    let url = handle.lan_url.clone().expect("host advertises a lan url");
    let (mut ws, _peer_id) = join(&url, &handle.session_id, "Nosy Player").await;

    // The GM rolls SECRET — the tool acks the numbers to the host only.
    let call = ToolCall {
        id: "tool-secret".to_string(),
        name: "roll_dice".to_string(),
        input: json!({ "expr": "1d20+5", "secret": true }),
    };
    let result =
        llm_tools::dispatch_tool_session(&call, &db, &registry, Some(&handle)).await.unwrap();
    let total = result["total"].as_i64().expect("host sees the secret total");
    assert!((6..=25).contains(&total));

    // …but the client's patch carries a redacted entry: no numbers.
    let frame = await_frame(&mut ws, "patch", |v| {
        v["kind"] == "chat_appended" && v["entry"]["text"] == "(secret GM roll)"
    })
    .await;
    assert!(
        frame["entry"]["roll"].is_null(),
        "secret roll must not sync numbers to clients: {frame}"
    );

    // …and the canonical doc (what a p2p peer would sync) is clean too.
    let snapshot = handle.snapshot_now().unwrap();
    assert_eq!(snapshot.chat.len(), 1);
    assert_eq!(snapshot.chat[0].text, "(secret GM roll)");
    assert!(
        snapshot.chat[0].roll.is_none(),
        "secret roll numbers must never live in the doc"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn client_update_sheet_targeting_gm_pc_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();

    let handle: Arc<SessionHandle> = Arc::new(
        host(tmp.path(), "guard-campaign", "Demo GM", None, no_adventures())
            .await
            .expect("host the table"),
    );

    // The GM creates their own character first, so the client's rejection
    // is authority-based (cross-player target), not "no such sheet".
    handle
        .intent(Intent::CreateCharacter {
            name: "GM PC".to_string(),
            color: "#8b5cf6".to_string(),
            character: sheet(),
        })
        .await
        .expect("GM creates own character");
    let gm_peer = handle.peer_id.clone();

    let url = handle.lan_url.clone().expect("host advertises a lan url");
    let (mut ws, _peer_id) = join(&url, &handle.session_id, "Sneaky Player").await;

    // The player tries to patch the GM's sheet.
    send_json(
        &mut ws,
        json!({
            "type": "intent",
            "intent": {
                "kind": "update_sheet",
                "peer_id": gm_peer,
                "patch": { "hp": 1 }
            }
        }),
    )
    .await;

    let err = await_frame(&mut ws, "error", |v| {
        v["rejected_intent"].is_object() && v["rejected_intent"]["kind"] == "update_sheet"
    })
    .await;
    assert!(
        err["message"].as_str().unwrap().contains("own sheet"),
        "expected cross-player rejection, got: {err}"
    );

    // Host state unchanged: the GM's sheet kept its hp.
    let snapshot = handle.snapshot_now().unwrap();
    let gm = snapshot.players.get(&gm_peer).expect("gm player present");
    let character = gm.character.as_ref().expect("gm has a character");
    assert_eq!(character.hp, 18, "gm sheet must be untouched");
    assert_eq!(character.max_hp, 18);
}
