//! LAN WebSocket bridge (CONTRACT § 4.3, feature `lan`).
//!
//! Plain RFC-6455 over `tokio::net::TcpListener` with JSON text frames.
//! Thin clients (mobile browsers) join via
//! `ws://<lan-ip>:<port>/<session_id>` and speak the same Intent /
//! PatchEvent serde shapes as the IPC path — their reducer is
//! patch-driven and never mutates locally.

use anyhow::{anyhow, bail, Result};
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use crate::session::doc::SessionEvent;
use crate::session::service::SessionService;
use crate::session::{Intent, Role, SessionPatchEvent, SessionPresenceEvent};

/// Canonical LAN port (CONTRACT § 2).
pub const LAN_PORT: u16 = 4780;

/// Client → host frames.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientFrame {
    Hello { session_id: String, name: String },
    Intent { intent: Intent },
}

/// Host → client frames. Patch/presence payloads are byte-identical to
/// the Tauri event payloads (same serde enums re-serialized).
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum HostFrame {
    Welcome {
        session_id: String,
        peer_id: String,
        role: Role,
    },
    Patch {
        #[serde(flatten)]
        event: SessionPatchEvent,
    },
    Presence {
        #[serde(flatten)]
        event: SessionPresenceEvent,
    },
    Error {
        message: String,
        rejected_intent: Option<Intent>,
    },
}

/// Accept TCP connections forever, upgrading each to a WS session.
/// Stops (closing every client socket) when the session shuts down.
pub async fn serve(listener: TcpListener, svc: Arc<SessionService>) -> Result<()> {
    let mut shutdown = svc.shutdown_rx();
    loop {
        let (stream, addr) = tokio::select! {
            acc = listener.accept() => acc.map_err(|e| anyhow!("lan accept: {e}"))?,
            res = shutdown.changed() => {
                if res.is_err() || *shutdown.borrow() {
                    return Ok(());
                }
                continue;
            }
        };
        tracing::debug!(%addr, "lan connection");
        let task_svc = svc.clone();
        let shutdown = shutdown.clone();
        let task = tokio::spawn(async move {
            let ws = match tokio_tungstenite::accept_async(stream).await {
                Ok(ws) => ws,
                Err(e) => {
                    tracing::debug!(error = %e, "lan ws handshake failed");
                    return;
                }
            };
            if let Err(e) = handle_socket(ws, task_svc, shutdown).await {
                tracing::debug!(error = %e, "lan session ended with error");
            }
        });
        svc.register_task(task);
    }
}

/// `ws://<lan-ip>:<port>/<session_id>` — the URL advertised to thin
/// clients. The LAN IP is discovered with a no-packet UDP `connect`.
pub fn advertised_addr(port: u16, session_id: &str) -> Result<String> {
    let ip = local_lan_ip().unwrap_or_else(|| "127.0.0.1".to_string());
    Ok(format!("ws://{ip}:{port}/{session_id}"))
}

fn local_lan_ip() -> Option<String> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    Some(sock.local_addr().ok()?.ip().to_string())
}

type WsSink = SplitSink<WebSocketStream<TcpStream>, Message>;

async fn send_frame(sink: &mut WsSink, frame: &HostFrame) -> Result<()> {
    let json = serde_json::to_string(frame)?;
    sink.send(Message::Text(json.into()))
        .await
        .map_err(|e| anyhow!("ws send: {e}"))?;
    Ok(())
}

async fn handle_socket(
    ws: WebSocketStream<TcpStream>,
    svc: Arc<SessionService>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    let (mut sink, mut stream) = ws.split();

    // --- hello handshake ---
    let hello_json = loop {
        match stream.next().await {
            Some(Ok(Message::Text(t))) => break t.to_string(),
            Some(Ok(Message::Close(_))) => bail!("closed before hello"),
            Some(Ok(_)) => continue, // pings are answered by tungstenite
            Some(Err(e)) => return Err(anyhow!("ws read: {e}")),
            None => bail!("stream ended before hello"),
        }
    };
    let hello: ClientFrame = match serde_json::from_str(&hello_json) {
        Ok(f) => f,
        Err(e) => {
            let _ = send_frame(
                &mut sink,
                &HostFrame::Error {
                    message: format!("malformed hello: {e}"),
                    rejected_intent: None,
                },
            )
            .await;
            let _ = sink.close().await;
            return Ok(());
        }
    };
    let ClientFrame::Hello { session_id, name } = hello else {
        let _ = send_frame(
            &mut sink,
            &HostFrame::Error { message: "expected hello first".into(), rejected_intent: None },
        )
        .await;
        let _ = sink.close().await;
        return Ok(());
    };
    let expected = svc.session_id();
    if session_id != expected {
        let _ = send_frame(
            &mut sink,
            &HostFrame::Error {
                message: format!("unknown session (wanted {expected})"),
                rejected_intent: None,
            },
        )
        .await;
        let _ = sink.close().await;
        return Ok(());
    }

    let minted = Uuid::new_v4().to_string();

    // Subscribe BEFORE joining so no broadcast is missed.
    let mut rx = svc.subscribe();

    // Register the peer (players map + presence, broadcast to all).
    // Roster semantics: a disconnected entry with the same name is
    // reattached — the minted id is then discarded in favor of the
    // existing identity (character included).
    let peer_id = match svc.peer_joined(&minted, &name, "lan") {
        Ok(id) => id,
        Err(e) => {
            let _ = send_frame(
                &mut sink,
                &HostFrame::Error {
                    message: format!("join failed: {e}"),
                    rejected_intent: None,
                },
            )
            .await;
            let _ = sink.close().await;
            return Ok(());
        }
    };

    send_frame(
        &mut sink,
        &HostFrame::Welcome { session_id: session_id.clone(), peer_id: peer_id.clone(), role: Role::Player },
    )
    .await?;
    // Snapshot patch right after welcome.
    let snapshot = svc.snapshot_now()?;
    send_frame(
        &mut sink,
        &HostFrame::Patch { event: SessionPatchEvent::snapshot(session_id.clone(), snapshot) },
    )
    .await?;

    // --- main loop: forward broadcasts, apply intents ---
    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(SessionEvent::Patch(event)) => {
                        send_frame(&mut sink, &HostFrame::Patch { event }).await?;
                    }
                    Ok(SessionEvent::Presence(event)) => {
                        send_frame(&mut sink, &HostFrame::Presence { event }).await?;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "lan client lagged; resending snapshot");
                        let snapshot = svc.snapshot_now()?;
                        send_frame(
                            &mut sink,
                            &HostFrame::Patch { event: SessionPatchEvent::snapshot(session_id.clone(), snapshot) },
                        )
                        .await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            msg = stream.next() => {
                match msg {
                    Some(Ok(Message::Text(t))) => {
                        match serde_json::from_str::<ClientFrame>(&t) {
                            Ok(ClientFrame::Intent { intent }) => {
                                // Authority is re-checked here (§ 4.4):
                                // LAN peers are always players.
                                match svc.apply(&peer_id, Role::Player, intent.clone()).await {
                                    Ok(_events) => {} // broadcast via the channel
                                    Err(e) => {
                                        // Error to the sender only, no broadcast.
                                        send_frame(
                                            &mut sink,
                                            &HostFrame::Error {
                                                message: e.to_string(),
                                                rejected_intent: Some(intent),
                                            },
                                        )
                                        .await?;
                                    }
                                }
                            }
                            Ok(ClientFrame::Hello { .. }) => {
                                send_frame(
                                    &mut sink,
                                    &HostFrame::Error {
                                        message: "already handshaked".into(),
                                        rejected_intent: None,
                                    },
                                )
                                .await?;
                            }
                            Err(e) => {
                                send_frame(
                                    &mut sink,
                                    &HostFrame::Error {
                                        message: format!("malformed frame: {e}"),
                                        rejected_intent: None,
                                    },
                                )
                                .await?;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => break,
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        tracing::debug!(error = %e, "lan ws error");
                        break;
                    }
                    None => break,
                }
            }
            res = shutdown.changed() => {
                // Session retired (leave / replace): close the client
                // socket so stale peers cannot keep mutating the doc.
                if res.is_err() || *shutdown.borrow() {
                    break;
                }
            }
        }
    }

    // Socket close → peer_left.
    let _ = svc.peer_left(&peer_id);
    let _ = sink.close().await;
    Ok(())
}
