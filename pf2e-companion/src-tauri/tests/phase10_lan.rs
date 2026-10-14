//! Phase 10 — LAN WebSocket bridge (CONTRACT § 6).
//!
//! `TcpListener` on 127.0.0.1:0, in-process tungstenite client:
//! hello → welcome → snapshot; chat intent broadcast to every client;
//! wrong-authority intent → `error` frame, host state unchanged.

#![cfg(feature = "lan")]

use futures_util::{SinkExt, StreamExt};
use pf2e_companion_lib::lan;
use pf2e_companion_lib::session::service::{no_adventures, SessionService};
use pf2e_companion_lib::session::{SessionDoc, SessionMeta, SharedDoc};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tokio_tungstenite::tungstenite::Message;

type ClientWs = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// One host service + LAN bridge on an ephemeral loopback port.
async fn setup() -> (Arc<SessionService>, u16) {
    setup_with_pin(None).await
}

/// Like [`setup`] but with a table PIN (CONTRACT-10_5 § 1).
async fn setup_with_pin(pin: Option<&str>) -> (Arc<SessionService>, u16) {
    let meta = SessionMeta {
        id: "lan-test".to_string(),
        name: "LAN Table".to_string(),
        gm_id: "gm-peer".to_string(),
        created_at: 1_700_000_000_000,
        adventure_slug: None,
        ended: false,
    };
    let svc = SessionService::new(
        SharedDoc::new(SessionDoc::new_session(meta)),
        None,
        no_adventures(),
        pin.map(str::to_string),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let s = svc.clone();
    tokio::spawn(async move {
        if let Err(e) = lan::serve(listener, s).await {
            eprintln!("lan serve ended: {e}");
        }
    });
    (svc, port)
}

async fn connect(port: u16, session_id: &str) -> ClientWs {
    let url = format!("ws://127.0.0.1:{port}/{session_id}");
    let (ws, _) = tokio_tungstenite::connect_async(url)
        .await
        .expect("tungstenite client connects");
    ws
}

async fn send_json(ws: &mut ClientWs, v: Value) {
    ws.send(Message::Text(v.to_string().into()))
        .await
        .expect("client send");
}

/// Read frames until one matches `type` and the predicate.
async fn await_frame(ws: &mut ClientWs, want_type: &str, pred: impl Fn(&Value) -> bool) -> Value {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
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

async fn hello(ws: &mut ClientWs, session_id: &str, name: &str) -> Value {
    send_json(ws, json!({ "type": "hello", "session_id": session_id, "name": name })).await;
    await_frame(ws, "welcome", |_| true).await
}

/// Hello with an explicit `pin` field (CONTRACT-10_5 § 1).
async fn hello_with_pin(ws: &mut ClientWs, session_id: &str, name: &str, pin: Option<&str>) {
    send_json(
        ws,
        json!({ "type": "hello", "session_id": session_id, "name": name, "pin": pin }),
    )
    .await;
}

/// Read until the socket closes; asserts an `error` frame with `message`
/// arrives first and that the connection then terminates.
async fn expect_error_then_close(ws: &mut ClientWs, want_message: &str) {
    let err = await_frame(ws, "error", |v| v["message"].as_str().unwrap_or("").contains(want_message))
        .await;
    assert_eq!(err["rejected_intent"], serde_json::Value::Null);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        match tokio::time::timeout_at(deadline, ws.next()).await {
            Err(_) => panic!("socket stayed open after PIN rejection"),
            Ok(None) | Ok(Some(Err(_))) | Ok(Some(Ok(Message::Close(_)))) => break,
            Ok(Some(Ok(_))) => continue,
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn hello_yields_welcome_snapshot_and_presence() {
    let (svc, port) = setup().await;
    let mut ws = connect(port, "lan-test").await;

    let welcome = hello(&mut ws, "lan-test", "Thin Client").await;
    assert_eq!(welcome["session_id"], "lan-test");
    assert_eq!(welcome["role"], "player");
    let peer_id = welcome["peer_id"].as_str().unwrap().to_string();
    assert!(!peer_id.is_empty());

    // Snapshot patch right after the welcome.
    let snap = await_frame(&mut ws, "patch", |v| v["kind"] == "snapshot").await;
    assert_eq!(snap["session_id"], "lan-test");
    assert_eq!(snap["snapshot"]["meta"]["id"], "lan-test");

    // peer_joined → player_upserted patch + presence event.
    let upsert = await_frame(&mut ws, "patch", |v| v["kind"] == "player_upserted").await;
    assert_eq!(upsert["peer_id"], peer_id.as_str());
    let presence = await_frame(&mut ws, "presence", |v| v["kind"] == "peer_joined").await;
    assert_eq!(presence["name"], "Thin Client");
    assert_eq!(presence["via"], "lan");

    // The host registered the peer.
    let snapshot = svc.snapshot_now().unwrap();
    assert!(snapshot.players.contains_key(&peer_id));
}

#[tokio::test(flavor = "multi_thread")]
async fn chat_intent_broadcasts_to_every_client() {
    let (svc, port) = setup().await;

    let mut a = connect(port, "lan-test").await;
    let wa = hello(&mut a, "lan-test", "Client A").await;
    assert_eq!(wa["role"], "player");
    let mut b = connect(port, "lan-test").await;
    let wb = hello(&mut b, "lan-test", "Client B").await;
    assert_eq!(wb["role"], "player");

    // A chats; both A and B must see the appended entry.
    send_json(
        &mut a,
        json!({ "type": "intent", "intent": { "kind": "chat", "text": "hello from the wire" } }),
    )
    .await;

    for ws in [&mut a, &mut b] {
        let frame = await_frame(ws, "patch", |v| {
            v["kind"] == "chat_appended" && v["entry"]["text"] == "hello from the wire"
        })
        .await;
        assert_eq!(frame["entry"]["kind"], "chat");
    }

    let snapshot = svc.snapshot_now().unwrap();
    assert_eq!(snapshot.chat.len(), 1);
    assert_eq!(snapshot.chat[0].text, "hello from the wire");
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_authority_intent_gets_error_and_state_unchanged() {
    let (svc, port) = setup().await;
    let mut ws = connect(port, "lan-test").await;
    hello(&mut ws, "lan-test", "Rebel").await;

    // set_initiative is GM-only; LAN peers are always players.
    send_json(
        &mut ws,
        json!({ "type": "intent", "intent": { "kind": "set_initiative", "order": ["sneaky"] } }),
    )
    .await;

    let err = await_frame(&mut ws, "error", |v| {
        v["rejected_intent"].is_object() && v["rejected_intent"]["kind"] == "set_initiative"
    })
    .await;
    assert!(
        err["message"].as_str().unwrap().contains("GM"),
        "error should say the intent is GM-only: {err}"
    );

    // Host state untouched: no initiative order, no chat.
    let snapshot = svc.snapshot_now().unwrap();
    assert!(snapshot.init.order.is_empty(), "init order must stay empty");
    assert!(snapshot.chat.is_empty(), "chat must stay empty");
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_session_id_in_hello_is_rejected() {
    let (_svc, port) = setup().await;
    let mut ws = connect(port, "lan-test").await;
    send_json(&mut ws, json!({ "type": "hello", "session_id": "bogus", "name": "X" })).await;
    let err = await_frame(&mut ws, "error", |_| true).await;
    assert!(
        err["message"].as_str().unwrap().contains("unknown session"),
        "expected unknown-session error, got: {err}"
    );
}

#[test]
fn advertised_addr_is_a_ws_url_with_the_session_path() {
    let url = lan::advertised_addr(4780, "my-sess").unwrap();
    assert!(url.starts_with("ws://"), "got {url}");
    assert!(url.contains(":4780/"), "got {url}");
    assert!(url.ends_with("/my-sess"), "got {url}");
}

// === Roster / lifecycle (review blockers) ===============================

#[tokio::test(flavor = "multi_thread")]
async fn reconnect_under_same_name_reattaches_with_character() {
    let (svc, port) = setup().await;

    // First connection: join + create a character.
    let (mut a, peer_id) = {
        let mut ws = connect(port, "lan-test").await;
        let welcome = hello(&mut ws, "lan-test", "Ruth").await;
        (ws, welcome["peer_id"].as_str().unwrap().to_string())
    };
    send_json(
        &mut a,
        json!({
            "type": "intent",
            "intent": {
                "kind": "create_character",
                "name": "Ruth",
                "color": "#fff",
                "character": {
                    "level": 1, "class": "Cleric", "hp": 16, "max_hp": 16,
                    "ac": 16, "conditions": [], "notes": ""
                }
            }
        }),
    )
    .await;
    let _ = await_frame(&mut a, "patch", |v| v["kind"] == "player_upserted"
        && v["player"]["character"].is_object())
    .await;

    // Drop the socket → disconnect (roster entry kept, marked away).
    drop(a);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = svc.snapshot_now().unwrap();
        if snapshot.players.get(&peer_id).map(|p| !p.connected).unwrap_or(false) {
            assert!(
                snapshot.players[&peer_id].character.is_some(),
                "character must survive the disconnect"
            );
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "peer never marked disconnected");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Reconnect with a NEW socket but the SAME name → reattached to the
    // original peer id, character intact.
    let mut b = connect(port, "lan-test").await;
    let welcome = hello(&mut b, "lan-test", "Ruth").await;
    let new_peer = welcome["peer_id"].as_str().unwrap();
    assert_eq!(new_peer, peer_id, "same-name reconnect must reattach");

    let snap_frame = await_frame(&mut b, "patch", |v| v["kind"] == "snapshot").await;
    let players = snap_frame["snapshot"]["players"].as_object().unwrap();
    let player = players.get(peer_id.as_str()).expect("roster entry present").as_object().unwrap();
    assert_eq!(player["name"], "Ruth");
    assert!(player["character"].is_object(), "character must be in the snapshot");
    assert_eq!(player["connected"], true, "reattached entry is connected");
}

#[tokio::test(flavor = "multi_thread")]
async fn session_shutdown_closes_lan_client_sockets() {
    let (svc, port) = setup().await;
    let mut ws = connect(port, "lan-test").await;
    hello(&mut ws, "lan-test", "Lingerer").await;

    // Retire the session (what session_leave / replace does).
    svc.shutdown();

    // The client socket must be closed: the next read terminates (close
    // frame / EOF / error) instead of hanging, so a post-leave intent
    // can never mutate the doc.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let closed = loop {
        match tokio::time::timeout_at(deadline, ws.next()).await {
            Err(_) => panic!("client socket stayed open after shutdown"),
            Ok(None) => break true,
            Ok(Some(Ok(Message::Close(_)))) => break true,
            Ok(Some(Err(_))) => break true,
            Ok(Some(Ok(_))) => continue,
        }
    };
    assert!(closed, "expected connection close");
    // A send after close is an error or the doc is simply unreachable.
    let send_result = ws.send(Message::Text(
        json!({ "type": "intent", "intent": { "kind": "chat", "text": "too late" } })
            .to_string()
            .into(),
    ))
    .await;
    if send_result.is_ok() {
        // If the frame made it into the socket, the server never applies
        // it — the handler is gone.
        let chat = svc.snapshot_now().unwrap().chat;
        assert!(chat.is_empty(), "post-shutdown intent must not mutate the doc");
    }
}

// === Table PIN (CONTRACT-10_5 § 1) =======================================

#[tokio::test(flavor = "multi_thread")]
async fn hello_with_wrong_pin_is_rejected_and_closed() {
    let (_svc, port) = setup_with_pin(Some("AB2345")).await;
    let mut ws = connect(port, "lan-test").await;
    hello_with_pin(&mut ws, "lan-test", "Intruder", Some("WRONG9")).await;
    expect_error_then_close(&mut ws, "bad or missing table PIN").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn hello_without_pin_is_rejected_and_closed() {
    let (_svc, port) = setup_with_pin(Some("AB2345")).await;
    let mut ws = connect(port, "lan-test").await;
    // No `pin` field at all.
    send_json(&mut ws, json!({ "type": "hello", "session_id": "lan-test", "name": "No Pin" }))
        .await;
    expect_error_then_close(&mut ws, "bad or missing table PIN").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn reattach_requires_matching_pin() {
    let (svc, port) = setup_with_pin(Some("AB2345")).await;

    // First join with the correct PIN, create a character.
    let (mut a, peer_id) = {
        let mut ws = connect(port, "lan-test").await;
        hello_with_pin(&mut ws, "lan-test", "Ruth", Some("AB2345")).await;
        let welcome = await_frame(&mut ws, "welcome", |_| true).await;
        (ws, welcome["peer_id"].as_str().unwrap().to_string())
    };
    send_json(
        &mut a,
        json!({
            "type": "intent",
            "intent": {
                "kind": "create_character",
                "name": "Ruth",
                "color": "#fff",
                "character": {
                    "level": 1, "class": "Cleric", "hp": 16, "max_hp": 16,
                    "ac": 16, "conditions": [], "notes": ""
                }
            }
        }),
    )
    .await;
    let _ = await_frame(&mut a, "patch", |v| v["kind"] == "player_upserted"
        && v["player"]["character"].is_object())
    .await;
    drop(a);
    // Wait for the disconnect to be processed.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = svc.snapshot_now().unwrap();
        if snapshot.players.get(&peer_id).map(|p| !p.connected).unwrap_or(false) {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "peer never marked disconnected");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Reconnect with the same name but a WRONG pin → rejected (the
    // character must not be reachable without pin + name).
    {
        let mut ws = connect(port, "lan-test").await;
        hello_with_pin(&mut ws, "lan-test", "Ruth", Some("NOPE99")).await;
        expect_error_then_close(&mut ws, "bad or missing table PIN").await;
    }
    // Roster untouched by the rejected attempt.
    let snapshot = svc.snapshot_now().unwrap();
    assert!(snapshot.players[&peer_id].character.is_some());
    assert!(!snapshot.players[&peer_id].connected);

    // Correct pin + name → reattached to the original identity.
    let mut b = connect(port, "lan-test").await;
    hello_with_pin(&mut b, "lan-test", "Ruth", Some("AB2345")).await;
    let snap_frame = await_frame(&mut b, "patch", |v| v["kind"] == "snapshot").await;
    let players = snap_frame["snapshot"]["players"].as_object().unwrap();
    let player = players.get(peer_id.as_str()).expect("roster entry present").as_object().unwrap();
    assert!(player["character"].is_object(), "reattached with character");
    assert_eq!(player["connected"], true);
}

#[tokio::test(flavor = "multi_thread")]
async fn session_without_pin_accepts_any_hello() {
    // Back-compat: pin-less sessions (SessionService::new with pin None)
    // accept a hello without any `pin` field.
    let (_svc, port) = setup().await;
    let mut ws = connect(port, "lan-test").await;
    let welcome = hello(&mut ws, "lan-test", "Old Client").await;
    assert_eq!(welcome["session_id"], "lan-test");
    let _ = await_frame(&mut ws, "patch", |v| v["kind"] == "snapshot").await;
}
