//! iroh p2p sync pump (CONTRACT § 4.2, feature `p2p`).
//!
//! `automerge::sync` over one long-lived bi-stream per peer pair on
//! ALPN `b"pf2e-table/1"`. Frames are `u32_le_len || sync::Message::encode()`.
//! The joiner opens the stream FIRST (the host's `accept_bi` blocks
//! until the joiner writes). Per-peer `sync::State`; after each receive
//! the pump regenerates until `generate_sync_message` yields None, then
//! idles on `select!` — incoming frames and local-change pokes both
//! restart the exchange, so both sides converge and stay converged.

use anyhow::{anyhow, Result};
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc, watch, Notify};

use crate::session::doc::{SessionEvent, SharedDoc};

/// Application protocol identifier for the live-table sync stream.
pub const ALPN: &[u8] = b"pf2e-table/1";

/// Hard cap on one sync frame (DoS guard: the length prefix is a
/// peer-controlled u32, i.e. up to 4 GiB).
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Bind a host endpoint and start accepting sync peers. Returns the
/// `EndpointTicket` string to share with joiners plus the accept-loop
/// task handle (register it so session shutdown can abort it).
pub async fn host_endpoint(
    doc: SharedDoc,
    events: broadcast::Sender<SessionEvent>,
    sync: Arc<Notify>,
    shutdown: watch::Receiver<bool>,
) -> Result<(String, tokio::task::JoinHandle<()>)> {
    let mut shutdown = shutdown;
    let ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .map_err(|e| anyhow!("iroh bind: {e}"))?;
    let ticket = iroh_tickets::endpoint::EndpointTicket::new(ep.addr()).to_string();

    let task = tokio::spawn(async move {
        loop {
            let incoming = tokio::select! {
                inc = ep.accept() => match inc {
                    Some(i) => i,
                    None => break,
                },
                res = shutdown.changed() => {
                    if res.is_err() || *shutdown.borrow() {
                        break;
                    }
                    continue;
                }
            };
            let conn = match incoming.await {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(error = %e, "iroh incoming connection failed");
                    continue;
                }
            };
            let doc = doc.clone();
            let events = events.clone();
            let sync = sync.clone();
            let shutdown = shutdown.clone();
            tokio::spawn(async move {
                match conn.accept_bi().await {
                    Ok((send, recv)) => {
                        if let Err(e) = pump(send, recv, doc, events, sync, shutdown).await {
                            tracing::debug!(error = %e, "iroh sync peer ended");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "iroh accept_bi failed");
                    }
                }
            });
        }
    });

    Ok((ticket, task))
}

/// Connect to a host via its `EndpointTicket` and start the sync pump.
/// Returns once the connection + bi-stream are open; the pump keeps
/// running in the background (its task handle is returned so callers
/// can abort it on session shutdown).
pub async fn connect_and_sync(
    ticket: &str,
    doc: SharedDoc,
    events: broadcast::Sender<SessionEvent>,
    sync: Arc<Notify>,
    shutdown: watch::Receiver<bool>,
) -> Result<tokio::task::JoinHandle<()>> {
    let ticket: iroh_tickets::endpoint::EndpointTicket =
        ticket.parse().map_err(|e| anyhow!("parse endpoint ticket: {e}"))?;
    let addr = ticket.endpoint_addr().clone();
    let ep = iroh::Endpoint::bind(iroh::endpoint::presets::N0)
        .await
        .map_err(|e| anyhow!("iroh bind: {e}"))?;
    let conn = ep
        .connect(addr, ALPN)
        .await
        .map_err(|e| anyhow!("iroh connect: {e}"))?;
    // Joiner opens the stream FIRST (§ 4.2).
    let (send, recv) = conn
        .open_bi()
        .await
        .map_err(|e| anyhow!("iroh open_bi: {e}"))?;
    Ok(tokio::spawn(async move {
        if let Err(e) = pump(send, recv, doc, events, sync, shutdown).await {
            tracing::debug!(error = %e, "iroh sync pump ended");
        }
    }))
}

/// The bidirectional sync exchange over one bi-stream. Also used
/// directly by the loopback tests.
pub async fn pump(
    mut send: iroh::endpoint::SendStream,
    mut recv: iroh::endpoint::RecvStream,
    doc: SharedDoc,
    events: broadcast::Sender<SessionEvent>,
    sync: Arc<Notify>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let state = Arc::new(Mutex::new(automerge::sync::State::new()));

    // Cancel-safe reader: parse length-prefixed frames on a dedicated
    // task and forward them over a channel (read errors included, so a
    // malformed frame fails the pump instead of looking like a clean
    // close).
    let (frame_tx, mut frame_rx) = mpsc::channel::<Result<Vec<u8>>>(64);
    tokio::spawn(async move {
        loop {
            match read_frame(&mut recv).await {
                Ok(Some(bytes)) => {
                    if frame_tx.send(Ok(bytes)).await.is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    let _ = frame_tx.send(Err(e)).await;
                    break;
                }
            }
        }
    });

    loop {
        // Send everything we have pending for this peer.
        loop {
            let message = {
                let mut st = state.lock().expect("sync state");
                doc.generate_sync_message(&mut st)
            };
            match message {
                Some(m) => write_frame(&mut send, &m.encode()).await?,
                None => break, // idle
            }
        }
        // Block until an incoming frame, a local-change poke, or
        // session shutdown.
        tokio::select! {
            frame = frame_rx.recv() => {
                match frame {
                    Some(Ok(bytes)) => {
                        let message = automerge::sync::Message::decode(&bytes)
                            .map_err(|e| anyhow!("decode sync frame: {e}"))?;
                        let mut st = state.lock().expect("sync state");
                        doc.receive_sync_message(&mut st, message, &events)?;
                    }
                    Some(Err(e)) => return Err(e),
                    None => return Ok(()), // peer closed the stream
                }
            }
            _ = sync.notified() => {
                // Local change — loop pass regenerates messages.
            }
            res = shutdown.changed() => {
                if res.is_err() || *shutdown.borrow() {
                    return Ok(());
                }
            }
        }
    }
}

async fn write_frame(send: &mut iroh::endpoint::SendStream, bytes: &[u8]) -> Result<()> {
    let len = u32::try_from(bytes.len()).map_err(|e| anyhow!("sync frame too large: {e}"))?;
    send.write_all(&len.to_le_bytes())
        .await
        .map_err(|e| anyhow!("write frame length: {e}"))?;
    send.write_all(bytes)
        .await
        .map_err(|e| anyhow!("write frame body: {e}"))?;
    Ok(())
}

async fn read_frame(recv: &mut iroh::endpoint::RecvStream) -> Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    if recv.read_exact(&mut len_buf).await.is_err() {
        return Ok(None); // closed
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(anyhow!(
            "sync frame of {len} bytes exceeds the {} byte cap",
            MAX_FRAME_BYTES
        ));
    }
    let mut buf = vec![0u8; len];
    recv.read_exact(&mut buf)
        .await
        .map_err(|e| anyhow!("read frame body: {e}"))?;
    Ok(Some(buf))
}

// === Loopback test helpers (phase10_p2p.rs) =============================

/// An offline endpoint: relays disabled, no address lookup, bound to
/// loopback only. Used by the loopback tests.
pub async fn loopback_endpoint() -> Result<iroh::Endpoint> {
    iroh::Endpoint::builder(iroh::endpoint::presets::N0)
        .relay_mode(iroh::RelayMode::Disabled)
        .clear_address_lookup()
        .alpns(vec![ALPN.to_vec()])
        .bind_addr("127.0.0.1:0")
        .map_err(|e| anyhow!("loopback bind addr: {e}"))?
        .bind()
        .await
        .map_err(|e| anyhow!("loopback bind: {e}"))
}

/// A dialable `EndpointAddr` for a loopback endpoint.
pub fn loopback_addr(ep: &iroh::Endpoint) -> iroh::EndpointAddr {
    let addrs: Vec<iroh::TransportAddr> =
        ep.addr().ip_addrs().cloned().map(iroh::TransportAddr::Ip).collect();
    iroh::EndpointAddr::from_parts(ep.id(), addrs)
}
