//! Host/peer session service (CONTRACT § 2 `session/service.rs`).
//!
//! Owns the [`SharedDoc`], fans PatchEvents out to every subscriber
//! (Tauri emitter, LAN bridge), persists the canonical automerge bytes
//! to `<vault>/campaigns/<id>/sessions/<id>.automerge` (debounced 2 s,
//! immediate on `EndSession`), and pokes the p2p sync pump on local
//! changes.

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, watch, Notify};
use uuid::Uuid;

use super::doc::{SessionEvent, SharedDoc, SessionMeta};
use super::{
    authorize, Intent, PatchEventPayload, Role, RollRecord, SessionPatchEvent,
    SessionSnapshot,
};
use crate::adventure::AdventureDef;

/// TS `SessionInfo`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SessionInfo {
    pub session_id: String,
    pub peer_id: String,
    pub role: Role,
    pub lan_url: Option<String>,
    pub p2p_ticket: Option<String>,
    pub name: String,
}

/// Resolves an adventure slug to its parsed def. The IPC layer installs
/// a DB-backed resolver; tests use stubs.
pub type AdventureResolver = Arc<dyn Fn(&str) -> Option<AdventureDef> + Send + Sync>;

/// A no-op resolver for peers and tests.
pub fn no_adventures() -> AdventureResolver {
    Arc::new(|_| None)
}

/// DB-backed resolver (host side).
pub fn db_adventures(db: Arc<crate::db::Db>) -> AdventureResolver {
    Arc::new(move |slug| crate::adventure::load_adventure_def(&db, slug).ok())
}

pub struct SessionService {
    doc: SharedDoc,
    events: broadcast::Sender<SessionEvent>,
    save_path: Option<PathBuf>,
    save_notify: Arc<Notify>,
    sync_notify: Arc<Notify>,
    adventures: AdventureResolver,
    /// Shutdown signal (LAN bridge, iroh accept loop, forwarder, saver).
    shutdown: watch::Sender<bool>,
    /// Tasks spawned for this session — aborted by [`SessionService::shutdown`].
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl SessionService {
    pub fn new(
        doc: SharedDoc,
        save_path: Option<PathBuf>,
        adventures: AdventureResolver,
    ) -> Arc<Self> {
        let (events, _) = broadcast::channel(512);
        let (shutdown, _) = watch::channel(false);
        Arc::new(SessionService {
            doc,
            events,
            save_path,
            save_notify: Arc::new(Notify::new()),
            sync_notify: Arc::new(Notify::new()),
            adventures,
            shutdown,
            tasks: Mutex::new(Vec::new()),
        })
    }

    /// A receiver that resolves once the session is retired.
    pub fn shutdown_rx(&self) -> watch::Receiver<bool> {
        self.shutdown.subscribe()
    }

    /// Track a session task so [`SessionService::shutdown`] can abort it.
    pub fn register_task(&self, task: tokio::task::JoinHandle<()>) {
        self.tasks.lock().expect("task registry").push(task);
    }

    /// Retire the session: signal every spawned task (LAN bridge client
    /// sockets, iroh accept loop + pumps, event forwarder, autosaver) to
    /// stop, then abort any that linger. Prevents stale sessions from
    /// accepting mutations or forwarding patches after `session_leave`.
    pub fn shutdown(&self) {
        let _ = self.shutdown.send(true);
        let tasks = std::mem::take(&mut *self.tasks.lock().expect("task registry"));
        for task in tasks {
            task.abort();
        }
    }

    pub fn session_id(&self) -> String {
        self.doc.session_id()
    }

    pub fn shared_doc(&self) -> SharedDoc {
        self.doc.clone()
    }

    pub fn events_sender(&self) -> broadcast::Sender<SessionEvent> {
        self.events.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.events.subscribe()
    }

    /// Poke the p2p sync pumps (called after every local change).
    pub fn sync_notify(&self) -> Arc<Notify> {
        self.sync_notify.clone()
    }

    pub fn notify_sync(&self) {
        self.sync_notify.notify_waiters();
    }

    /// Authority-checked intent application: authorize → apply →
    /// persist (debounced) → broadcast.
    pub async fn apply(
        &self,
        actor: &str,
        role: Role,
        intent: Intent,
    ) -> Result<Vec<SessionPatchEvent>> {
        self.apply_tracked(actor, role, intent).await.map(|(events, _)| events)
    }

    /// Like [`SessionService::apply`] but also returns the numeric roll
    /// record when the intent was a dice roll — the host-side ack for
    /// secret GM rolls (the doc itself carries no numbers).
    pub async fn apply_tracked(
        &self,
        actor: &str,
        role: Role,
        intent: Intent,
    ) -> Result<(Vec<SessionPatchEvent>, Option<RollRecord>)> {
        authorize(role, actor, &intent).map_err(|e| anyhow!(e))?;
        let (events, roll) = if let Intent::LoadAdventure { slug } = &intent {
            let def = (self.adventures)(slug).ok_or_else(|| {
                anyhow!(super::IntentError::invalid(format!(
                    "unknown adventure slug `{slug}`"
                )))
            })?;
            (self.doc.with(|d| d.apply_adventure(&def, slug))?, None)
        } else {
            let events = self.doc.with(|d| d.apply_intent(actor, role, &intent))?;
            let roll = self.doc.with(|d| d.take_last_roll());
            (events, roll)
        };
        for event in &events {
            let _ = self.events.send(SessionEvent::Patch(event.clone()));
        }
        if matches!(intent, Intent::EndSession) {
            self.save_now().context("save on end_session")?;
            // Poke the p2p pumps so peers learn the session ended.
            self.notify_sync();
        } else {
            self.save_notify.notify_one();
            self.notify_sync();
        }
        Ok((events, roll))
    }

    /// Current snapshot (JSON mirror of § 4).
    pub fn snapshot_now(&self) -> Result<SessionSnapshot> {
        self.doc.with(|d| d.snapshot())
    }

    /// Write the canonical session bytes now (atomic tmp+rename).
    pub fn save_now(&self) -> Result<()> {
        let Some(path) = &self.save_path else { return Ok(()) };
        let bytes = self.doc.with(|d| d.save());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        let tmp = path.with_extension("automerge.tmp");
        std::fs::write(&tmp, &bytes)
            .with_context(|| format!("write {}", tmp.display()))?;
        std::fs::rename(&tmp, path)
            .with_context(|| format!("rename {}", path.display()))?;
        Ok(())
    }

    /// Debounced autosave loop: after a change, wait 2 s (draining
    /// further pokes) then write (CONTRACT § 1.3). Exits when the
    /// session shuts down.
    pub fn spawn_saver(self: &Arc<Self>) {
        let svc = self.clone();
        let mut shutdown = svc.shutdown_rx();
        let registrar = self.clone();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = svc.save_notify.notified() => {
                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_secs(2)) => {
                                if let Err(e) = svc.save_now() {
                                    tracing::warn!(error = %e, "session autosave failed");
                                }
                            }
                            res = shutdown.changed() => {
                                if res.is_err() || *shutdown.borrow() { break; }
                            }
                        }
                    }
                    res = shutdown.changed() => {
                        if res.is_err() || *shutdown.borrow() { break; }
                    }
                }
            }
        });
        registrar.register_task(task);
    }

    /// Broadcast an error PatchEvent (authority rejections etc.).
    pub fn broadcast_error(&self, message: String) {
        let _ = self.events.send(SessionEvent::Patch(SessionPatchEvent::new(
            self.session_id(),
            PatchEventPayload::Error { message },
        )));
    }

    /// A peer joined (any transport). Roster semantics: a disconnected
    /// entry with the same name is reattached (character included), so
    /// reconnects are lossless; otherwise a fresh entry is created.
    /// Returns the effective peer id. Broadcasts presence +
    /// player_upserted.
    pub fn peer_joined(&self, peer_id: &str, name: &str, via: &str) -> Result<String> {
        let color = color_for(peer_id);
        let effective = self.doc.with(|d| d.join_player(peer_id, name, &color));
        let snapshot = self.snapshot_now()?;
        let player = snapshot.players.get(&effective).cloned().unwrap_or(super::PlayerInfo {
            name: name.to_string(),
            character: None,
            color,
            connected: true,
        });
        let session_id = self.session_id();
        let _ = self.events.send(SessionEvent::Patch(SessionPatchEvent::new(
            &session_id,
            PatchEventPayload::PlayerUpserted { peer_id: effective.clone(), player },
        )));
        let _ = self.events.send(SessionEvent::Presence(super::SessionPresenceEvent {
            session_id,
            payload: super::PresenceEventPayload::PeerJoined {
                peer_id: effective.clone(),
                name: name.to_string(),
                via: via.to_string(),
            },
        }));
        self.save_notify.notify_one();
        self.notify_sync();
        Ok(effective)
    }

    /// A peer left: the roster entry is kept (marked disconnected — the
    /// character survives for reattachment); broadcasts the updated
    /// player entry + presence.
    pub fn peer_left(&self, peer_id: &str) -> Result<()> {
        self.doc.with(|d| d.mark_disconnected(peer_id));
        let snapshot = self.snapshot_now()?;
        let player = snapshot.players.get(peer_id).cloned();
        let session_id = self.session_id();
        if let Some(player) = player {
            let _ = self.events.send(SessionEvent::Patch(SessionPatchEvent::new(
                &session_id,
                PatchEventPayload::PlayerUpserted { peer_id: peer_id.to_string(), player },
            )));
        }
        let _ = self.events.send(SessionEvent::Presence(super::SessionPresenceEvent {
            session_id,
            payload: super::PresenceEventPayload::PeerLeft { peer_id: peer_id.to_string() },
        }));
        self.save_notify.notify_one();
        self.notify_sync();
        Ok(())
    }
}

/// Deterministic pleasant color for a peer id.
fn color_for(peer_id: &str) -> String {
    const PALETTE: [&str; 8] = [
        "#ef4444", "#f97316", "#eab308", "#22c55e", "#06b6d4", "#3b82f6", "#8b5cf6", "#ec4899",
    ];
    let hash = peer_id.bytes().fold(0usize, |acc, b| acc.wrapping_mul(31).wrapping_add(b as usize));
    PALETTE[hash % PALETTE.len()].to_string()
}

// === Handle + registry ==================================================

/// Caller-facing handle for one joined session (host GM, or peer).
#[derive(Clone)]
pub struct SessionHandle {
    pub session_id: String,
    pub role: Role,
    pub peer_id: String,
    pub name: String,
    pub lan_url: Option<String>,
    pub p2p_ticket: Option<String>,
    svc: Arc<SessionService>,
}

impl SessionHandle {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        svc: Arc<SessionService>,
        session_id: String,
        role: Role,
        peer_id: String,
        name: String,
        lan_url: Option<String>,
        p2p_ticket: Option<String>,
    ) -> Arc<Self> {
        Arc::new(SessionHandle {
            session_id,
            role,
            peer_id,
            name,
            lan_url,
            p2p_ticket,
            svc,
        })
    }

    /// Authorize → apply → persist → broadcast.
    pub async fn intent(&self, intent: Intent) -> Result<()> {
        self.svc.apply(&self.peer_id, self.role, intent).await.map(|_| ())
    }

    /// Like [`SessionHandle::intent`] but returns the derived
    /// PatchEvents plus the numeric roll record when the intent was a
    /// dice roll (the host-side ack for secret GM rolls; used by the
    /// LLM `roll_dice` tool).
    pub async fn intent_events(
        &self,
        intent: Intent,
    ) -> Result<(Vec<SessionPatchEvent>, Option<RollRecord>)> {
        self.svc.apply_tracked(&self.peer_id, self.role, intent).await
    }

    pub fn snapshot_now(&self) -> Result<SessionSnapshot> {
        self.svc.snapshot_now()
    }

    pub fn service(&self) -> Arc<SessionService> {
        self.svc.clone()
    }
}

/// One active session per process.
#[derive(Clone)]
pub struct ActiveSession {
    pub service: Arc<SessionService>,
    pub handle: Arc<SessionHandle>,
    pub info: SessionInfo,
}

pub struct SessionRegistry(pub Mutex<Option<ActiveSession>>);

impl SessionRegistry {
    pub fn new() -> Self {
        SessionRegistry(Mutex::new(None))
    }

    /// Install a session, gracefully retiring any previous one (its
    /// spawned tasks are shut down so stale LAN/p2p clients can no
    /// longer mutate the old doc or forward patches into the new UI).
    pub fn install(&self, session: ActiveSession) {
        let old = self.0.lock().expect("registry lock").replace(session);
        if let Some(old) = old {
            let _ = old.service.save_now();
            let _ = old.service.peer_left(&old.handle.peer_id);
            old.service.shutdown();
        }
    }

    pub fn take(&self) -> Option<ActiveSession> {
        let old = self.0.lock().expect("registry lock").take();
        if let Some(old) = &old {
            old.service.shutdown();
        }
        old
    }

    pub fn get(&self) -> Option<ActiveSession> {
        self.0.lock().expect("registry lock").clone()
    }

    pub fn handle(&self) -> Option<Arc<SessionHandle>> {
        self.get().map(|s| s.handle)
    }
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// === Host / join / load ==================================================

/// Reject anything that escapes the campaign root (mirrors
/// vault_write::safe_segment).
fn safe_segment(seg: &str) -> Result<()> {
    if seg.is_empty()
        || seg.contains('/')
        || seg.contains('\\')
        || seg == ".."
        || seg.starts_with('.')
    {
        bail!("unsafe path segment: {seg:?}");
    }
    Ok(())
}

/// Bring up the full host stack: doc, persistence, LAN bridge (feature
/// `lan`), iroh endpoint (feature `p2p`), autosave, GM presence.
pub async fn host(
    vault: &Path,
    campaign: &str,
    name: &str,
    adventure: Option<&str>,
    adventures: AdventureResolver,
) -> Result<SessionHandle> {
    safe_segment(campaign)?;
    let session_id = Uuid::new_v4().to_string();
    let peer_id = Uuid::new_v4().to_string();
    let meta = SessionMeta {
        id: session_id.clone(),
        name: name.to_string(),
        gm_id: peer_id.clone(),
        created_at: super::doc::now_ms(),
        adventure_slug: None,
    };
    let doc = SharedDoc::new(super::SessionDoc::new_session(meta));

    let dir = vault.join("campaigns").join(campaign).join("sessions");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("create {}", dir.display()))?;
    let save_path = dir.join(format!("{session_id}.automerge"));

    let svc = SessionService::new(doc, Some(save_path), adventures);
    svc.save_now().context("initial session save")?;
    svc.spawn_saver();

    let lan_url = start_lan(&svc).await;
    let p2p_ticket = start_p2p(&svc).await;

    svc.peer_joined(&peer_id, name, "ipc")?;
    if let Some(slug) = adventure {
        svc.apply(&peer_id, Role::Gm, Intent::LoadAdventure { slug: slug.to_string() })
            .await?;
    }
    let handle = SessionHandle::new(
        svc.clone(),
        svc.session_id(),
        Role::Gm,
        peer_id,
        name.to_string(),
        lan_url,
        p2p_ticket,
    );
    Ok((*handle).clone())
}

/// Join via an iroh endpoint ticket (feature `p2p`). Desktop peers get
/// a Player-role handle whose doc syncs with the host.
#[cfg(feature = "p2p")]
pub async fn join_peer(
    ticket: &str,
    name: &str,
    adventures: AdventureResolver,
) -> Result<SessionHandle> {
    let peer_id = Uuid::new_v4().to_string();
    let doc = SharedDoc::new(super::SessionDoc::blank());
    let svc = SessionService::new(doc, None, adventures);

    let pump_task = crate::net::connect_and_sync(
        ticket,
        svc.shared_doc(),
        svc.events_sender(),
        svc.sync_notify(),
        svc.shutdown_rx(),
    )
    .await
    .context("connect to host")?;
    svc.register_task(pump_task);

    // Wait for the initial sync to deliver the session meta.
    let session_id = wait_for_meta(&svc).await?;

    // Presence (via "p2p") without requiring a CreateCharacter first;
    // a same-name disconnected roster entry is reattached.
    let effective = svc.peer_joined(&peer_id, name, "p2p")?;

    let handle = SessionHandle::new(
        svc.clone(),
        session_id,
        Role::Player,
        effective,
        name.to_string(),
        None,
        None,
    );
    Ok((*handle).clone())
}

#[cfg(feature = "p2p")]
async fn wait_for_meta(svc: &Arc<SessionService>) -> Result<String> {
    for _ in 0..100 {
        let snapshot = svc.snapshot_now()?;
        if !snapshot.meta.id.is_empty() {
            return Ok(snapshot.meta.id);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    bail!("timed out waiting for session meta from host")
}

/// Resume a persisted session as GM.
pub async fn load(
    vault: &Path,
    campaign: &str,
    session_id: &str,
    name: &str,
    adventures: AdventureResolver,
) -> Result<SessionHandle> {
    safe_segment(campaign)?;
    safe_segment(session_id)?;
    let path = vault
        .join("campaigns")
        .join(campaign)
        .join("sessions")
        .join(format!("{session_id}.automerge"));
    let bytes = std::fs::read(&path)
        .with_context(|| format!("read {}", path.display()))?;
    let doc = super::SessionDoc::load(&bytes)?;
    let real_session_id = doc.session_id().to_string();
    let peer_id = Uuid::new_v4().to_string();
    let svc = SessionService::new(SharedDoc::new(doc), Some(path), adventures);
    svc.spawn_saver();

    let lan_url = start_lan(&svc).await;
    let p2p_ticket = start_p2p(&svc).await;
    // Reattach by name when resuming (a same-name disconnected roster
    // entry keeps its identity + character), else fresh GM entry.
    let peer_id = svc.peer_joined(&peer_id, name, "ipc")?;

    let handle = SessionHandle::new(
        svc.clone(),
        real_session_id,
        Role::Gm,
        peer_id,
        name.to_string(),
        lan_url,
        p2p_ticket,
    );
    Ok((*handle).clone())
}

/// LAN bridge lifecycle (feature `lan`). Binds the canonical port 4780,
/// falling back to an ephemeral port when it is taken.
#[cfg(feature = "lan")]
async fn start_lan(svc: &Arc<SessionService>) -> Option<String> {
    use tokio::net::TcpListener;
    let session_id = svc.session_id();
    let listener = match TcpListener::bind(("0.0.0.0", crate::lan::LAN_PORT)).await {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!(error = %e, "port {} busy; binding ephemeral", crate::lan::LAN_PORT);
            TcpListener::bind("0.0.0.0:0").await.ok()?
        }
    };
    let port = listener.local_addr().ok()?.port();
    let url = crate::lan::advertised_addr(port, &session_id).ok()?;
    let svc2 = svc.clone();
    let task = tokio::spawn(async move {
        if let Err(e) = crate::lan::serve(listener, svc2).await {
            tracing::warn!(error = %e, "lan bridge stopped");
        }
    });
    svc.register_task(task);
    Some(url)
}

#[cfg(not(feature = "lan"))]
async fn start_lan(_svc: &Arc<SessionService>) -> Option<String> {
    None
}

/// iroh host endpoint (feature `p2p`). Returns the endpoint ticket.
#[cfg(feature = "p2p")]
async fn start_p2p(svc: &Arc<SessionService>) -> Option<String> {
    match crate::net::host_endpoint(
        svc.shared_doc(),
        svc.events_sender(),
        svc.sync_notify(),
        svc.shutdown_rx(),
    )
    .await
    {
        Ok((ticket, task)) => {
            svc.register_task(task);
            Some(ticket)
        }
        Err(e) => {
            tracing::warn!(error = %e, "p2p host endpoint failed");
            None
        }
    }
}

#[cfg(not(feature = "p2p"))]
async fn start_p2p(_svc: &Arc<SessionService>) -> Option<String> {
    None
}
