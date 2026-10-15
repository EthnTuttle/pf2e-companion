//! Phase 10 — iroh p2p loopback convergence (CONTRACT § 6).
//!
//! `RelayMode::Disabled`, loopback `EndpointAddr`, no network beyond
//! 127.0.0.1. **Offline guard**: these tests are skipped unless
//! `PF2E_NET_TESTS=1` — the default `cargo test` run needs zero network.

#![cfg(feature = "p2p")]

use pf2e_companion_lib::net;
use pf2e_companion_lib::session::doc::{SessionEvent, SharedDoc};
use pf2e_companion_lib::session::{Intent, Role, SessionDoc, SessionMeta};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Notify};

fn meta() -> SessionMeta {
    SessionMeta {
        id: "p2p-loopback".to_string(),
        name: "P2P Table".to_string(),
        gm_id: "gm-peer".to_string(),
        created_at: 1_700_000_000_000,
        adventure_slug: None,
        ended: false,
    }
}

struct Peer {
    doc: SharedDoc,
    events: broadcast::Sender<SessionEvent>,
    sync: Arc<Notify>,
    shutdown: tokio::sync::watch::Receiver<bool>,
    // Kept alive so `shutdown.changed()` doesn't fire Err (sender-dropped)
    // in the pumps under test.
    _shutdown_tx: tokio::sync::watch::Sender<bool>,
}

fn peer(doc: SharedDoc) -> Peer {
    let (events, _) = broadcast::channel(128);
    let (_shutdown_tx, shutdown) = tokio::sync::watch::channel(false);
    Peer { doc, events, sync: Arc::new(Notify::new()), shutdown, _shutdown_tx }
}

async fn wait_until(deadline_s: u64, pred: impl Fn() -> bool, what: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(deadline_s);
    while !pred() {
        if tokio::time::Instant::now() >= deadline {
            panic!("timed out waiting for {what}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn loopback_peers_converge_and_resync_on_change() {
    if std::env::var("PF2E_NET_TESTS").as_deref() != Ok("1") {
        return;
    }

    // Host side: a doc with content, an accept loop pumping every peer.
    let host = peer(SharedDoc::new(SessionDoc::new_session(meta())));
    host.doc.with(|d| {
        d.apply_intent("gm-peer", Role::Gm, &Intent::Chat { text: "welcome".into() })
            .unwrap();
    });
    let host_ep = net::loopback_endpoint().await.expect("host loopback endpoint");
    let host_addr = net::loopback_addr(&host_ep);
    {
        let host_doc = host.doc.clone();
        let host_events = host.events.clone();
        let host_sync = host.sync.clone();
        let host_shutdown = host.shutdown.clone();
        tokio::spawn(async move {
            while let Some(incoming) = host_ep.accept().await {
                let conn = match incoming.await {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let (send, recv) = match conn.accept_bi().await {
                    Ok(x) => x,
                    Err(_) => continue,
                };
                let doc = host_doc.clone();
                let events = host_events.clone();
                let sync = host_sync.clone();
                let shutdown = host_shutdown.clone();
                tokio::spawn(async move {
                    if let Err(e) = net::pump(send, recv, doc, events, sync, shutdown).await {
                        eprintln!("host pump ended: {e}");
                    }
                });
            }
        });
    }

    // Joiner side: blank doc, opens the bi-stream first (§ 4.2).
    let joiner = peer(SharedDoc::new(SessionDoc::blank()));
    // Subscribe BEFORE the sync starts — broadcast only delivers to
    // existing receivers.
    let mut rx = joiner.events.subscribe();
    let joiner_ep = net::loopback_endpoint().await.expect("joiner loopback endpoint");
    let conn = joiner_ep
        .connect(host_addr, net::ALPN)
        .await
        .expect("joiner connects over loopback");
    let (send, recv) = conn.open_bi().await.expect("joiner opens bi-stream");
    {
        let doc = joiner.doc.clone();
        let events = joiner.events.clone();
        let sync = joiner.sync.clone();
        let shutdown = joiner.shutdown.clone();
        tokio::spawn(async move {
            if let Err(e) = net::pump(send, recv, doc, events, sync, shutdown).await {
                eprintln!("joiner pump ended: {e}");
            }
        });
    }

    // Initial sync delivers the whole session to the joiner.
    wait_until(30, || {
        joiner.doc.with(|d| d.snapshot().unwrap().meta.id == "p2p-loopback")
    }, "joiner to receive session meta")
    .await;
    let host_snap = host.doc.with(|d| d.snapshot().unwrap());
    let joiner_snap = joiner.doc.with(|d| d.snapshot().unwrap());
    assert_eq!(host_snap, joiner_snap, "snapshots must converge");
    assert_eq!(joiner_snap.chat.len(), 1);

    // Change-triggered re-sync: the host changes + pokes its pump…
    host.doc.with(|d| {
        d.apply_intent("gm-peer", Role::Gm, &Intent::Chat { text: "round two".into() })
            .unwrap();
    });
    host.sync.notify_waiters();
    wait_until(30, || joiner.doc.with(|d| d.snapshot().unwrap().chat.len()) == 2,
        "joiner to receive the host's second chat")
    .await;

    // …and the joiner's change propagates back to the host.
    joiner.doc.with(|d| {
        d.apply_intent("gm-peer", Role::Gm, &Intent::Chat { text: "from the edge".into() })
            .unwrap();
    });
    joiner.sync.notify_waiters();
    wait_until(30, || host.doc.with(|d| d.snapshot().unwrap().chat.len()) == 3,
        "host to receive the joiner's chat")
    .await;

    // Full convergence both ways.
    let host_snap = host.doc.with(|d| d.snapshot().unwrap());
    let joiner_snap = joiner.doc.with(|d| d.snapshot().unwrap());
    assert_eq!(host_snap, joiner_snap);
    assert_eq!(joiner_snap.chat[2].text, "from the edge");

    // The joiner's event bus derived patches from the merged changes.
    let mut kinds = Vec::new();
    while let Ok(event) = rx.try_recv() {
        if let SessionEvent::Patch(p) = event {
            kinds.push(p.payload);
        }
    }
    assert!(
        kinds.iter().any(|p| matches!(p, pf2e_companion_lib::session::PatchEventPayload::ChatAppended { .. })),
        "expected ChatAppended in the joiner's merge diff, got {kinds:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn oversized_sync_frame_is_rejected_not_allocated() {
    if std::env::var("PF2E_NET_TESTS").as_deref() != Ok("1") {
        return;
    }

    // A malicious peer announces a ~4 GiB frame via the length prefix.
    // The pump must fail the stream instead of allocating the buffer.
    let host = peer(SharedDoc::new(SessionDoc::new_session(meta())));
    let host_ep = net::loopback_endpoint().await.expect("host loopback endpoint");
    let host_addr = net::loopback_addr(&host_ep);

    let (result_tx, result_rx) = tokio::sync::oneshot::channel::<String>();
    {
        let doc = host.doc.clone();
        let events = host.events.clone();
        let sync = host.sync.clone();
        let shutdown = host.shutdown.clone();
        tokio::spawn(async move {
            let mut result_tx = Some(result_tx);
            while let Some(incoming) = host_ep.accept().await {
                let Ok(conn) = incoming.await else { continue };
                let Ok((send, recv)) = conn.accept_bi().await else { continue };
                let doc = doc.clone();
                let events = events.clone();
                let sync = sync.clone();
                let shutdown = shutdown.clone();
                let result_tx = result_tx.take();
                tokio::spawn(async move {
                    if let Some(result_tx) = result_tx {
                        match net::pump(send, recv, doc, events, sync, shutdown).await {
                            Ok(()) => {
                                let _ = result_tx.send("pump ended cleanly".to_string());
                            }
                            Err(e) => {
                                let _ = result_tx.send(e.to_string());
                            }
                        }
                    }
                });
            }
        });
    }

    let attacker_ep = net::loopback_endpoint().await.expect("attacker loopback endpoint");
    let conn = attacker_ep.connect(host_addr, net::ALPN).await.expect("attacker connects");
    let (mut send, _recv) = conn.open_bi().await.expect("attacker opens bi");
    // Length prefix 0xFFFFFFF0 (≈4 GiB) + a partial body.
    send.write_all(&0xFFFFFFF0u32.to_le_bytes()).await.unwrap();
    send.write_all(&[0xAB; 64]).await.unwrap();

    let outcome = tokio::time::timeout(Duration::from_secs(30), result_rx)
        .await
        .expect("timeout waiting for pump result")
        .expect("pump task dropped result channel");
    assert!(
        outcome.contains("exceeds") && outcome.contains("cap"),
        "expected frame-cap rejection, got: {outcome}"
    );
}

// === WAN/relay convergence (CONTRACT-11 §2; manual, env-gated) ===========
//
// Same shape as `loopback_peers_converge_and_resync_on_change` but the
// endpoints keep the default n0 relay map (`RelayMode::Default`, no
// `clear_address_lookup`), so the joiner reaches the host through real
// relay infra. Gated behind `PF2E_NET_WAN=1`; the whole converge flow
// is capped at 10s so a sandboxed network fails loudly, never hangs.

/// Bind a relay-enabled endpoint. Unlike `net::loopback_endpoint` this
/// keeps the default relay map + address lookup (needs outbound QUIC).
async fn wan_endpoint() -> iroh::Endpoint {
    iroh::Endpoint::builder(iroh::endpoint::presets::N0)
        .relay_mode(iroh::RelayMode::Default)
        .alpns(vec![net::ALPN.to_vec()])
        .bind()
        .await
        .expect("wan endpoint bind")
}

#[tokio::test(flavor = "multi_thread")]
async fn relay_peers_converge_via_n0_relay() {
    if std::env::var("PF2E_NET_WAN").as_deref() != Ok("1") {
        return;
    }

    let flow = async {
        // Host side: doc with content, accept loop pumping every peer.
        let host = peer(SharedDoc::new(SessionDoc::new_session(meta())));
        host.doc.with(|d| {
            d.apply_intent("gm-peer", Role::Gm, &Intent::Chat { text: "welcome".into() })
                .unwrap();
        });
        let host_ep = wan_endpoint().await;
        // Relay-only ticket: clone the host's EndpointAddr and strip
        // every direct/IP address, keeping ONLY the relay URL —
        // same-host loopback addrs would otherwise let the dial bypass
        // the n0 relay entirely. The relay address appears once the
        // endpoint's relay connection is established, so poll for it.
        let relay_addr = {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
            loop {
                let mut addr = host_ep.addr();
                addr.addrs.retain(iroh::TransportAddr::is_relay);
                if !addr.addrs.is_empty() {
                    break addr;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "host endpoint never advertised a relay address (n0 relay unreachable?)"
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        };
        let ticket =
            iroh_tickets::endpoint::EndpointTicket::new(relay_addr).to_string();
        {
            let host_doc = host.doc.clone();
            let host_events = host.events.clone();
            let host_sync = host.sync.clone();
            let host_shutdown = host.shutdown.clone();
            tokio::spawn(async move {
                while let Some(incoming) = host_ep.accept().await {
                    let conn = match incoming.await {
                        Ok(c) => c,
                        Err(_) => continue,
                    };
                    let (send, recv) = match conn.accept_bi().await {
                        Ok(x) => x,
                        Err(_) => continue,
                    };
                    let doc = host_doc.clone();
                    let events = host_events.clone();
                    let sync = host_sync.clone();
                    let shutdown = host_shutdown.clone();
                    tokio::spawn(async move {
                        if let Err(e) = net::pump(send, recv, doc, events, sync, shutdown).await
                        {
                            eprintln!("host pump ended: {e}");
                        }
                    });
                }
            });
        }

        // Joiner side: blank doc, reaches the host via the relay infra.
        let joiner = peer(SharedDoc::new(SessionDoc::blank()));
        // Subscribe BEFORE the sync starts — broadcast only delivers to
        // existing receivers.
        let mut rx = joiner.events.subscribe();
        let ticket: iroh_tickets::endpoint::EndpointTicket =
            ticket.parse().expect("parse endpoint ticket");
        let joiner_ep = wan_endpoint().await;
        // The ticket carries ONLY the relay URL (no direct addrs), so
        // this dial must go through the n0 relay for real.
        let conn = joiner_ep
            .connect(ticket.endpoint_addr().clone(), net::ALPN)
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "PF2E_NET_WAN: relayed connect failed — network sandbox may \
                     block UDP/443 QUIC to n0 relays (connect: {e})"
                )
            });
        let (send, recv) = conn.open_bi().await.expect("joiner opens bi-stream");
        {
            let doc = joiner.doc.clone();
            let events = joiner.events.clone();
            let sync = joiner.sync.clone();
            let shutdown = joiner.shutdown.clone();
            tokio::spawn(async move {
                if let Err(e) = net::pump(send, recv, doc, events, sync, shutdown).await {
                    eprintln!("joiner pump ended: {e}");
                }
            });
        }

        // Initial sync delivers the whole session to the joiner.
        wait_until(5, || {
            joiner.doc.with(|d| d.snapshot().unwrap().meta.id == "p2p-loopback")
        }, "joiner to receive session meta")
        .await;
        let host_snap = host.doc.with(|d| d.snapshot().unwrap());
        let joiner_snap = joiner.doc.with(|d| d.snapshot().unwrap());
        assert_eq!(host_snap, joiner_snap, "snapshots must converge via relay");
        assert_eq!(joiner_snap.chat.len(), 1);

        // Change-triggered re-sync: host → joiner …
        host.doc.with(|d| {
            d.apply_intent("gm-peer", Role::Gm, &Intent::Chat { text: "round two".into() })
                .unwrap();
        });
        host.sync.notify_waiters();
        wait_until(5, || joiner.doc.with(|d| d.snapshot().unwrap().chat.len()) == 2,
            "joiner to receive the host's second chat")
        .await;

        // … and joiner → host.
        joiner.doc.with(|d| {
            d.apply_intent("gm-peer", Role::Gm, &Intent::Chat { text: "from the edge".into() })
                .unwrap();
        });
        joiner.sync.notify_waiters();
        wait_until(5, || host.doc.with(|d| d.snapshot().unwrap().chat.len()) == 3,
            "host to receive the joiner's chat")
        .await;

        // Full convergence both ways.
        let host_snap = host.doc.with(|d| d.snapshot().unwrap());
        let joiner_snap = joiner.doc.with(|d| d.snapshot().unwrap());
        assert_eq!(host_snap, joiner_snap, "final snapshots must match via relay");
        assert_eq!(joiner_snap.chat[2].text, "from the edge");

        // The joiner's event bus derived patches from the merged changes.
        let mut kinds = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let SessionEvent::Patch(event) = event {
                kinds.push(event.payload);
            }
        }
        assert!(
            kinds.iter().any(|p| matches!(
                p,
                pf2e_companion_lib::session::PatchEventPayload::ChatAppended { .. }
            )),
            "expected ChatAppended in the joiner's merge diff, got {kinds:?}"
        );
    };

    tokio::time::timeout(Duration::from_secs(10), flow)
        .await
        .unwrap_or_else(|_| {
            panic!(
                "PF2E_NET_WAN: relayed connect failed — network sandbox may \
                 block UDP/443 QUIC to n0 relays"
            )
        });
}
