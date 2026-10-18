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

/// Client → host frames. `Hello.pin` is additive (CONTRACT-10_5 § 1):
/// required when the session holds a table PIN, ignored otherwise.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientFrame {
    Hello {
        session_id: String,
        name: String,
        #[serde(default)]
        pin: Option<String>,
    },
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
/// clients (lobby display, copy-invite, deep links all read this one
/// value; nothing frontend-side reassembles it). F-R2: the address comes
/// from ranked interface enumeration (`pick_lan_ipv4`); the historical
/// 8.8.8.8 UDP probe survives only as last-resort fallback.
pub fn advertised_addr(port: u16, session_id: &str) -> Result<String> {
    let ip = advertised_ip(enumerate_candidates(), legacy_udp_probe);
    Ok(format!("ws://{ip}:{port}/{session_id}"))
}

/// One IPv4 address on one interface, reduced to what advertisement
/// ranking needs. Synthesizable in unit tests — ranking never touches
/// the OS interface list itself. IPv6 is out of scope by construction:
/// LAN play advertises a v4 `ws://` URL (CONTRACT § 4.3) and home Wi-Fi
/// is dual-stack; a v6-only host enumerates zero candidates and degrades
/// to the probe/`127.0.0.1` path with a warning — never a crash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LanCandidate {
    /// OS interface name (`wlp2s0`, `en0`, `wg0-mullvad`, …). Tunnel
    /// filtering by name is a heuristic: names are per-OS conventions.
    pub name: String,
    pub ip: std::net::Ipv4Addr,
    pub prefixlen: u8,
    pub is_up: bool,
}

/// Snapshot the machine's IPv4 interface addresses. An empty vec (with a
/// warning) when enumeration itself fails — the caller then falls back
/// to the legacy probe, so behavior can only improve, never regress.
fn enumerate_candidates() -> Vec<LanCandidate> {
    match if_addrs::get_if_addrs() {
        Ok(ifaces) => ifaces
            .iter()
            .filter_map(|iface| match &iface.addr {
                if_addrs::IfAddr::V4(v4) => Some(LanCandidate {
                    name: iface.name.clone(),
                    ip: v4.ip,
                    prefixlen: v4.prefixlen,
                    is_up: iface.is_oper_up(),
                }),
                if_addrs::IfAddr::V6(_) => None,
            })
            .collect(),
        Err(e) => {
            tracing::warn!(error = %e, "interface enumeration failed; using legacy UDP probe");
            Vec::new()
        }
    }
}

/// Tunnel/virtual interface name prefixes, matched case-insensitively.
/// Heuristic only — interface naming is a per-OS convention, so the
/// RFC1918 preference order in `rank_class` is the second line of
/// defense when a tunnel dodges the filter (e.g. a non-/32 10/8 VPN).
const TUNNEL_NAME_PREFIXES: &[&str] = &[
    "tun", "tap", "wg", "utun", "gif", "stf", "ipsec", "ppp", "vpn",
    "tailscale", "ts-", "zt", "nebula", "outline",
    "docker", "veth", "br-", "virbr", "podman", "cni", "flannel",
];

fn is_tunnel_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    TUNNEL_NAME_PREFIXES.iter().any(|p| n.starts_with(p))
}

/// 100.64.0.0/10 — carrier-grade NAT (Tailscale tailnet addrs, cellular
/// uplinks). Never the network a phone on Wi-Fi sits on.
fn is_cgnat(ip: std::net::Ipv4Addr) -> bool {
    let o = ip.octets();
    o[0] == 100 && (o[1] & 0xC0) == 64
}

/// Lower is better. Home Wi-Fi is overwhelmingly 192.168/16; VPN
/// tunnels that slip the name filter are overwhelmingly 10/8 or CGNAT —
/// so private classes rank in that order, and any RFC1918 beats a public
/// address (phones can't dial the host's WAN IP through NAT anyway).
fn rank_class(ip: std::net::Ipv4Addr) -> u8 {
    let o = ip.octets();
    if o[0] == 192 && o[1] == 168 {
        0
    } else if o[0] == 172 && (16..32).contains(&o[1]) {
        1
    } else if o[0] == 10 {
        2
    } else {
        3
    }
}

/// Pure ranking: the address a phone on the same LAN can reach, or None
/// when nothing qualifies (caller's cue to fall back to the probe).
/// Excludes: down interfaces, /32 host routes (the Mullvad F-R2 shape),
/// loopback, link-local 169.254/16, CGNAT 100.64/10, unspecified,
/// multicast/experimental, and tunnel-named interfaces. Deterministic:
/// class rank, then interface name, then address.
pub(crate) fn pick_lan_ipv4(candidates: &[LanCandidate]) -> Option<std::net::Ipv4Addr> {
    let mut usable: Vec<&LanCandidate> = candidates
        .iter()
        .filter(|c| c.is_up)
        .filter(|c| (1..32).contains(&c.prefixlen))
        .filter(|c| {
            let ip = c.ip;
            !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_unspecified()
                && !ip.is_broadcast()
                && !ip.is_multicast()
                && ip.octets()[0] < 240
                && !is_cgnat(ip)
        })
        .filter(|c| !is_tunnel_name(&c.name))
        .collect();
    usable.sort_by(|a, b| {
        rank_class(a.ip)
            .cmp(&rank_class(b.ip))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.ip.cmp(&b.ip))
    });
    usable.first().map(|c| c.ip)
}

/// Resolve the advertised IP: ranked enumeration first, legacy probe as
/// fallback (with a warning, since on a VPN'd host the probe can still
/// return the tunnel address), `127.0.0.1` as the historical last resort.
fn advertised_ip(
    candidates: Vec<LanCandidate>,
    legacy_probe: impl FnOnce() -> Option<std::net::Ipv4Addr>,
) -> std::net::Ipv4Addr {
    if let Some(ip) = pick_lan_ipv4(&candidates) {
        return ip;
    }
    tracing::warn!(
        "no viable LAN interface found (VPN tunnels and down/link-local \
         interfaces are excluded); falling back to the 8.8.8.8 UDP probe — \
         on a VPN'd host the advertised address may still be unreachable"
    );
    legacy_probe().unwrap_or(std::net::Ipv4Addr::LOCALHOST)
}

/// Pre-F-R2 behavior, kept verbatim for the fallback path: a no-packet
/// UDP `connect` to a public address, then read back the chosen source.
/// It follows the host's policy routing — on a VPN'd host that is the
/// tunnel address (the F-R2 bug), which is why it is only a fallback.
fn legacy_udp_probe() -> Option<std::net::Ipv4Addr> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    match sock.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) => Some(ip),
        std::net::IpAddr::V6(_) => None,
    }
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
    let ClientFrame::Hello { session_id, name, pin } = hello else {
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
    // Table PIN (CONTRACT-10_5 § 1): gates first joins AND same-name
    // reattach equally (pin + name required to reattach). Sessions
    // without a pin accept any hello (back-compat).
    if let Some(expected_pin) = svc.pin() {
        if pin.as_deref() != Some(expected_pin) {
            let _ = send_frame(
                &mut sink,
                &HostFrame::Error {
                    message: "bad or missing table PIN".to_string(),
                    rejected_intent: None,
                },
            )
            .await;
            let _ = sink.close().await;
            return Ok(());
        }
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
    // Server-initiated keepalive: browsers auto-pong, so dead thin
    // clients surface as closed sockets instead of silent staleness
    // (CONTRACT-10_5 § 4).
    let mut ping_tick = tokio::time::interval(std::time::Duration::from_secs(15));
    ping_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                sink.send(Message::Ping(Vec::new().into()))
                    .await
                    .map_err(|e| anyhow!("ws ping: {e}"))?;
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn cand(name: &str, ip: &str, prefixlen: u8, is_up: bool) -> LanCandidate {
        LanCandidate {
            name: name.to_string(),
            ip: ip.parse().unwrap(),
            prefixlen,
            is_up,
        }
    }

    // (a) The F-R2 shape itself: real Wi-Fi next to a Mullvad-style
    // tunnel /32 on 10/8. Wi-Fi must win even though 10/8 is RFC1918.
    #[test]
    fn wifi_beats_tunnel_slash32() {
        let cands = vec![
            cand("wg0-mullvad", "10.146.185.179", 32, true),
            cand("wlp2s0", "192.168.1.172", 24, true),
        ];
        assert_eq!(pick_lan_ipv4(&cands), Some(Ipv4Addr::new(192, 168, 1, 172)));
    }

    // A tunnel with a *non*-/32 RFC1918 subnet is still excluded by the
    // (case-insensitive) name heuristic; the 192.168 preference is the
    // second line of defense if it dodges both.
    #[test]
    fn tunnel_names_excluded_case_insensitive() {
        let cands = vec![
            cand("WG0", "10.8.0.1", 24, true),
            cand("Tailscale0", "192.168.50.5", 24, true),
            cand("UTUN9", "172.16.3.2", 24, true),
            cand("enp3s0", "192.168.1.10", 24, true),
        ];
        assert_eq!(pick_lan_ipv4(&cands), Some(Ipv4Addr::new(192, 168, 1, 10)));
    }

    // (b) Only a tunnel exists → ranking yields nothing and the legacy
    // probe is used (warning is logged by `advertised_ip`). Probe
    // failure degrades to loopback, the historical last resort.
    #[test]
    fn tunnel_only_falls_back_to_probe() {
        let cands = vec![cand("tun0", "10.64.0.2", 32, true)];
        assert_eq!(pick_lan_ipv4(&cands), None);
        assert_eq!(
            advertised_ip(cands.clone(), || Some(Ipv4Addr::new(192, 168, 1, 99))),
            Ipv4Addr::new(192, 168, 1, 99)
        );
        assert_eq!(advertised_ip(cands, || None), Ipv4Addr::LOCALHOST);
    }

    // (c) Link-local 169.254/16 never gets advertised — it only appears
    // when DHCP failed, so a phone could never reach it meaningfully.
    #[test]
    fn link_local_excluded() {
        let only_ll = vec![cand("en0", "169.254.10.10", 16, true)];
        assert_eq!(pick_lan_ipv4(&only_ll), None);
        let with_wifi = vec![
            cand("en0", "169.254.10.10", 16, true),
            cand("en1", "192.168.1.20", 24, true),
        ];
        assert_eq!(pick_lan_ipv4(&with_wifi), Some(Ipv4Addr::new(192, 168, 1, 20)));
    }

    // (d) IPv6-only host: enumeration records no v4 candidates at all
    // (V6 addrs are dropped in `enumerate_candidates`), so this is an
    // empty list → no ranked pick → probe → and the v4-only probe fails
    // on a v6-only host too, yielding loopback with warnings. No crash,
    // no v6 literal in a `ws://` URL. Supporting LAN play over IPv6 is
    // out of scope; see plan doc § non-goals.
    #[test]
    fn ipv6_only_degrades_to_loopback_via_probe() {
        let no_v4: Vec<LanCandidate> = Vec::new();
        assert_eq!(pick_lan_ipv4(&no_v4), None);
        assert_eq!(advertised_ip(no_v4, || None), Ipv4Addr::LOCALHOST);
    }

    // Down interfaces (e.g. unplugged dock ethernet still holding its
    // lease) must not win over the live one.
    #[test]
    fn down_interface_excluded() {
        let cands = vec![
            cand("enp0s31f6", "192.168.1.5", 24, false),
            cand("wlp2s0", "192.168.1.172", 24, true),
        ];
        assert_eq!(pick_lan_ipv4(&cands), Some(Ipv4Addr::new(192, 168, 1, 172)));
        assert_eq!(pick_lan_ipv4(&cands[..1]), None);
    }

    // CGNAT (Tailscale tailnet 100.x, cellular) is excluded; when both
    // ethernet and Wi-Fi offer RFC1918, the 192.168/16 home class wins,
    // then 172.16/12, then 10/8.
    #[test]
    fn private_class_ordering_and_cgnat() {
        let cands = vec![
            cand("tailscale0", "100.64.1.1", 32, true),
            cand("corp-eth", "10.20.30.40", 16, true),
            cand("vmnet8", "172.16.99.1", 24, true),
            cand("wlp2s0", "192.168.8.2", 24, true),
        ];
        assert_eq!(pick_lan_ipv4(&cands), Some(Ipv4Addr::new(192, 168, 8, 2)));
        assert_eq!(
            pick_lan_ipv4(&cands[..3]),
            Some(Ipv4Addr::new(172, 16, 99, 1))
        );
        assert_eq!(pick_lan_ipv4(&cands[1..2]), Some(Ipv4Addr::new(10, 20, 30, 40)));
        assert_eq!(pick_lan_ipv4(&cands[..1]), None);
    }
}
