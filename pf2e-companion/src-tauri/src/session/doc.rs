//! Automerge session document (CONTRACT § 4.1).
//!
//! Top-level keys: `meta` (map, incl. `schema_version: 1`), `players`
//! (map by peer_id), `chat` (append-only list), `init` (map), `scene`
//! (map incl. `tokens` map), `handouts` (map). Optional strings are
//! stored as `""` (automerge has no null scalar in wide use) and mapped
//! back to `None` on read. Decoders tolerate missing keys with defaults
//! (v1 forward-compat).

use anyhow::{anyhow, Context, Result};
use automerge::sync::SyncDoc;
use automerge::transaction::Transactable;
use automerge::{
    sync, AutoCommit, ChangeHash, ObjId, ObjType, ReadDoc, Value, ROOT,
};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

use super::{
    authorize, diff_snapshots, ChatEntry, ChatKind, CharacterSheet, Grid, HandoutState,
    Intent, IntentError, PatchEventPayload, PlayerInfo, Role, RollRecord,
    SessionPatchEvent, SessionSnapshot, SessionMetaSnapshot, SheetPatch, Token,
};
use crate::adventure::AdventureDef;
use crate::dice;

/// Events fanned out to every subscriber (Tauri emitter, WS bridge, …).
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)] // snapshot patches dominate; fine on the broadcast path
pub enum SessionEvent {
    Patch(SessionPatchEvent),
    Presence(super::SessionPresenceEvent),
}

pub type SessionMeta = SessionMetaSnapshot;

/// Unix millis.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// The automerge-backed session record. Session doc bytes are the
/// canonical session record (SQLite-derived/files-canonical discipline).
pub struct SessionDoc {
    doc: AutoCommit,
    session_id: String,
    /// In-memory only (never synced): the numeric result of the most
    /// recent roll, so secret GM rolls can be acked to the host without
    /// storing the numbers in the shared doc.
    last_roll: Option<RollRecord>,
}

impl SessionDoc {
    /// Create a fresh v1 session document.
    pub fn new_session(meta: SessionMeta) -> Self {
        let mut doc = AutoCommit::new();
        let meta_obj = doc.put_object(ROOT, "meta", ObjType::Map).expect("root meta");
        doc.put(&meta_obj, "schema_version", 1i64).expect("schema_version");
        doc.put(&meta_obj, "id", meta.id.as_str()).expect("id");
        doc.put(&meta_obj, "name", meta.name.as_str()).expect("name");
        doc.put(&meta_obj, "gm_id", meta.gm_id.as_str()).expect("gm_id");
        doc.put(&meta_obj, "created_at", meta.created_at).expect("created_at");
        doc.put(&meta_obj, "adventure_slug", "").expect("adventure_slug");
        doc.put(&meta_obj, "ended", false).expect("ended");

        doc.put_object(ROOT, "players", ObjType::Map).expect("players");
        doc.put_object(ROOT, "chat", ObjType::List).expect("chat");

        let init = doc.put_object(ROOT, "init", ObjType::Map).expect("init");
        doc.put_object(&init, "order", ObjType::List).expect("init.order");
        doc.put(&init, "active_idx", 0i64).expect("active_idx");
        doc.put(&init, "round", 0i64).expect("round");

        let scene = doc.put_object(ROOT, "scene", ObjType::Map).expect("scene");
        let grid = doc.put_object(&scene, "grid", ObjType::Map).expect("grid");
        doc.put(&grid, "w", 24i64).expect("w");
        doc.put(&grid, "h", 24i64).expect("h");
        doc.put(&grid, "size", 48i64).expect("size");
        doc.put(&scene, "background_entity_id", "").expect("background");
        doc.put_object(&scene, "tokens", ObjType::Map).expect("tokens");

        doc.put_object(ROOT, "handouts", ObjType::Map).expect("handouts");

        let mut out = SessionDoc { doc, session_id: meta.id, last_roll: None };
        out.upsert_player(&meta.gm_id, &meta.name, "#8b5cf6");
        out
    }

    /// A blank document for a joining peer (populated by sync).
    pub fn blank() -> Self {
        SessionDoc { doc: AutoCommit::new(), session_id: String::new(), last_roll: None }
    }

    /// Load a saved session from its canonical bytes.
    pub fn load(bytes: &[u8]) -> Result<Self> {
        let doc = AutoCommit::load(bytes).context("load automerge session")?;
        let mut out = SessionDoc { doc, session_id: String::new(), last_roll: None };
        out.session_id = out.snapshot()?.meta.id;
        Ok(out)
    }

    /// Canonical save bytes.
    pub fn save(&mut self) -> Vec<u8> {
        self.doc.save()
    }

    /// Current document heads.
    pub fn heads(&mut self) -> Vec<ChangeHash> {
        self.doc.get_heads()
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Authority-checked intent application. Returns the derived
    /// PatchEvents (snapshot pre/post diff, CONTRACT § 4.2).
    pub fn apply_intent(
        &mut self,
        actor: &str,
        role: Role,
        intent: &Intent,
    ) -> Result<Vec<SessionPatchEvent>> {
        authorize(role, actor, intent).map_err(|e| anyhow!(e))?;
        let pre = self.snapshot()?;
        self.mutate(actor, role, intent)?;
        let post = self.snapshot()?;
        let mut events = diff_snapshots(&self.session_id, &pre, &post);
        if matches!(intent, Intent::EndSession) {
            events.push(SessionPatchEvent::new(
                &self.session_id,
                PatchEventPayload::SessionEnded,
            ));
        }
        Ok(events)
    }

    /// Full adventure population (CONTRACT § 5): slug, encounter-npc
    /// tokens, handout registration, and the scene-1 read-aloud system
    /// chat entry. Callers must have authorized `LoadAdventure` (GM).
    pub fn apply_adventure(
        &mut self,
        def: &AdventureDef,
        slug: &str,
    ) -> Result<Vec<SessionPatchEvent>> {
        let pre = self.snapshot()?;
        {
            let doc = &mut self.doc;
            let meta = child(doc, &ROOT, "meta")
                .ok_or_else(|| anyhow!("missing meta"))?;
            doc.put(&meta, "adventure_slug", slug)?;

            // Tokens from every encounter scene. Every rostered slug gets
            // a token (label from its sidecar when resolvable, else the
            // slug); ids are suffixed with a per-slug index so duplicate
            // slugs across scenes never collapse.
            let scene = ensure_map(doc, &ROOT, "scene");
            let tokens = ensure_map(doc, &scene, "tokens");
            let mut slot = 0i64;
            let mut per_slug: std::collections::BTreeMap<String, usize> =
                std::collections::BTreeMap::new();
            for scene_def in &def.scenes {
                let Some(encounter) = &scene_def.encounter else { continue };
                for npc_slug in &encounter.npcs {
                    let statblock = def
                        .npcs
                        .iter()
                        .find(|n| &n.slug == npc_slug)
                        .map(|n| &n.statblock)
                        .or_else(|| def.roster.get(npc_slug));
                    let label = statblock
                        .and_then(|sb| sb.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(npc_slug);
                    let index = per_slug.entry(npc_slug.clone()).or_insert(0);
                    let token_id = format!("npc-{npc_slug}-{index}");
                    *index += 1;
                    if !key_exists(doc, &tokens, &token_id) {
                        let t = doc.put_object(&tokens, &token_id, ObjType::Map)?;
                        doc.put(&t, "token_id", token_id.as_str())?;
                        doc.put(&t, "owner", Self::snapshot_gm_id(doc))?;
                        doc.put(&t, "entity_id", "")?;
                        doc.put(&t, "pc", "")?;
                        doc.put(&t, "x", 3.0 + (slot % 8) as f64 * 2.0)?;
                        doc.put(&t, "y", 3.0 + (slot / 8) as f64 * 2.0)?;
                        doc.put(&t, "label", label)?;
                        slot += 1;
                    }
                }
            }

            // Handouts (registered with their declared visibility).
            let handouts = ensure_map(doc, &ROOT, "handouts");
            for handout in &def.handouts {
                if !key_exists(doc, &handouts, &handout.id) {
                    let h = doc.put_object(&handouts, &handout.id, ObjType::Map)?;
                    doc.put(&h, "entity_id", def.id.as_str())?;
                    doc.put(&h, "visible_to", handout.visible_to.as_str())?;
                }
            }

            // System chat entry with the first scene's read-aloud.
            let chat = ensure_list(doc, &ROOT, "chat");
            let text = def
                .scenes
                .first()
                .and_then(|s| s.read_aloud.clone())
                .unwrap_or_else(|| format!("{} begins.", def.title));
            let entry = doc.insert_object(&chat, doc.length(&chat), ObjType::Map)?;
            doc.put(&entry, "ts", now_ms())?;
            doc.put(&entry, "from", "system")?;
            doc.put(&entry, "kind", "system")?;
            doc.put(&entry, "text", text.as_str())?;
            doc.put(&entry, "has_roll", false)?;
        }
        let post = self.snapshot()?;
        Ok(diff_snapshots(&self.session_id, &pre, &post))
    }

    fn snapshot_gm_id(doc: &AutoCommit) -> String {
        child(doc, &ROOT, "meta")
            .and_then(|m| get_str(doc, &m, "gm_id"))
            .unwrap_or_default()
    }

    fn mutate(&mut self, actor: &str, role: Role, intent: &Intent) -> Result<()> {
        let last_roll = self.mutate_doc(actor, role, intent)?;
        self.last_roll = last_roll;
        Ok(())
    }

    /// Doc mutation; returns the roll record when the intent was a dice
    /// roll (for the host-side ack of secret rolls).
    fn mutate_doc(
        &mut self,
        actor: &str,
        role: Role,
        intent: &Intent,
    ) -> Result<Option<RollRecord>> {
        let doc = &mut self.doc;
        let invalid = |m: &str| anyhow!(IntentError::invalid(m));
        let mut rolled = None;
        match intent {
            Intent::CreateCharacter { name, color, character } => {
                let players = ensure_map(doc, &ROOT, "players");
                let p = ensure_map(doc, &players, actor);
                doc.put(&p, "name", name.as_str())?;
                doc.put(&p, "color", color.as_str())?;
                doc.delete(&p, "character").ok();
                let c = doc.put_object(&p, "character", ObjType::Map)?;
                put_sheet(doc, &c, character)?;
            }
            Intent::UpdateSheet { peer_id, patch } => {
                let target = peer_id.clone().unwrap_or_else(|| actor.to_string());
                if role != Role::Gm && target != actor {
                    return Err(anyhow!(IntentError::forbidden(
                        "players may only update their own sheet"
                    )));
                }
                let players = ensure_map(doc, &ROOT, "players");
                let p = child(doc, &players, &target)
                    .ok_or_else(|| invalid("unknown player; create a character first"))?;
                let c = child(doc, &p, "character")
                    .ok_or_else(|| invalid("player has no character yet"))?;
                put_sheet_patch(doc, &c, patch)?;
            }
            Intent::RollDice { expr, secret } => {
                let spec = dice::parse(expr).map_err(|e| invalid(&e.to_string()))?;
                let secret = *secret && role == Role::Gm;
                let outcome = dice::roll(&spec);
                let record = RollRecord {
                    expr: outcome.expr.clone(),
                    total: outcome.total,
                    rolls: outcome.rolls.clone(),
                    kept: outcome.kept.clone(),
                };
                // Secret rolls are semantically secret: the synced doc
                // carries NO numbers — the numeric result is acked to the
                // host out-of-band via `take_last_roll` instead.
                let text = if secret {
                    "(secret GM roll)".to_string()
                } else {
                    format!("{} = {}", outcome.expr, outcome.total)
                };
                append_chat(doc, ChatEntry {
                    ts: now_ms(),
                    from: actor.to_string(),
                    kind: ChatKind::Roll,
                    text,
                    roll: if secret { None } else { Some(record.clone()) },
                })?;
                rolled = Some(record);
            }
            Intent::Chat { text } => {
                append_chat(doc, ChatEntry {
                    ts: now_ms(),
                    from: actor.to_string(),
                    kind: ChatKind::Chat,
                    text: text.clone(),
                    roll: None,
                })?;
            }
            Intent::RevealHandout { handout_id } => {
                let handouts = ensure_map(doc, &ROOT, "handouts");
                let h = child(doc, &handouts, handout_id)
                    .ok_or_else(|| invalid("unknown handout"))?;
                doc.put(&h, "visible_to", "all")?;
            }
            Intent::SetInitiative { order } => {
                let init = ensure_map(doc, &ROOT, "init");
                doc.delete(&init, "order").ok();
                let list = doc.put_object(&init, "order", ObjType::List)?;
                for peer in order {
                    doc.insert(&list, doc.length(&list), peer.as_str())?;
                }
                doc.put(&init, "active_idx", 0i64)?;
                if !order.is_empty() {
                    let round = get_i64(doc, &init, "round").unwrap_or(0);
                    if round == 0 {
                        doc.put(&init, "round", 1i64)?;
                    }
                }
            }
            Intent::AdvanceInitiative => {
                let init = ensure_map(doc, &ROOT, "init");
                let order = child(doc, &init, "order")
                    .ok_or_else(|| invalid("no initiative order set"))?;
                let len = doc.length(&order) as i64;
                if len == 0 {
                    return Err(invalid("no initiative order set"));
                }
                let active = get_i64(doc, &init, "active_idx").unwrap_or(0);
                let next = active + 1;
                if next >= len {
                    doc.put(&init, "active_idx", 0i64)?;
                    let round = get_i64(doc, &init, "round").unwrap_or(0);
                    doc.put(&init, "round", round + 1)?;
                } else {
                    doc.put(&init, "active_idx", next)?;
                }
            }
            Intent::MoveToken { token_id, x, y } => {
                let scene = ensure_map(doc, &ROOT, "scene");
                let tokens = ensure_map(doc, &scene, "tokens");
                let t = child(doc, &tokens, token_id)
                    .ok_or_else(|| invalid("unknown token"))?;
                if role != Role::Gm {
                    let owner = get_str(doc, &t, "owner").unwrap_or_default();
                    if owner != actor {
                        return Err(anyhow!(IntentError::forbidden(
                            "players may only move their own tokens"
                        )));
                    }
                }
                doc.put(&t, "x", *x)?;
                doc.put(&t, "y", *y)?;
            }
            Intent::AddToken { token } => {
                let scene = ensure_map(doc, &ROOT, "scene");
                let tokens = ensure_map(doc, &scene, "tokens");
                doc.delete(&tokens, token.token_id.as_str()).ok();
                let t = doc.put_object(&tokens, token.token_id.as_str(), ObjType::Map)?;
                put_token(doc, &t, token)?;
            }
            Intent::RemoveToken { token_id } => {
                let scene = ensure_map(doc, &ROOT, "scene");
                let tokens = ensure_map(doc, &scene, "tokens");
                if !key_exists(doc, &tokens, token_id) {
                    return Err(invalid("unknown token"));
                }
                doc.delete(&tokens, token_id.as_str())?;
            }
            Intent::SetScene { grid, background_entity_id } => {
                let scene = ensure_map(doc, &ROOT, "scene");
                let g = ensure_map(doc, &scene, "grid");
                doc.put(&g, "w", grid.w)?;
                doc.put(&g, "h", grid.h)?;
                doc.put(&g, "size", grid.size)?;
                doc.put(
                    &scene,
                    "background_entity_id",
                    background_entity_id.clone().unwrap_or_default().as_str(),
                )?;
            }
            Intent::LoadAdventure { slug } => {
                let meta = ensure_map(doc, &ROOT, "meta");
                doc.put(&meta, "adventure_slug", slug.as_str())?;
            }
            Intent::EndSession => {
                let meta = ensure_map(doc, &ROOT, "meta");
                doc.put(&meta, "ended", true)?;
                // Clear presence (characters stay on the roster).
                let players = ensure_map(doc, &ROOT, "players");
                for key in keys(doc, &players) {
                    if let Some(p) = child(doc, &players, &key) {
                        let _ = doc.put(&p, "connected", false);
                    }
                }
            }
        }
        Ok(rolled)
    }

    /// Upsert a player presence entry (no character).
    pub fn upsert_player(&mut self, peer_id: &str, name: &str, color: &str) {
        let doc = &mut self.doc;
        let players = ensure_map(doc, &ROOT, "players");
        let p = ensure_map(doc, &players, peer_id);
        let _ = doc.put(&p, "name", name);
        let _ = doc.put(&p, "color", color);
        let _ = doc.put(&p, "connected", true);
        let _ = doc.put(&p, "has_character", false);
    }

    /// A peer (re)joined. When a disconnected roster entry with the same
    /// name exists, it is reattached (identity + character kept) and its
    /// peer id returned; otherwise a fresh entry under `peer_id` is
    /// created. This is what makes reconnects lossless.
    pub fn join_player(&mut self, peer_id: &str, name: &str, color: &str) -> String {
        let doc = &mut self.doc;
        let players = ensure_map(doc, &ROOT, "players");
        for key in keys(doc, &players) {
            let Some(p) = child(doc, &players, &key) else { continue };
            let same_name = get_str(doc, &p, "name").as_deref() == Some(name);
            let disconnected = !get_bool(doc, &p, "connected").unwrap_or(true);
            if same_name && disconnected {
                let _ = doc.put(&p, "connected", true);
                return key;
            }
        }
        let p = ensure_map(doc, &players, peer_id);
        let _ = doc.put(&p, "name", name);
        let _ = doc.put(&p, "color", color);
        let _ = doc.put(&p, "connected", true);
        peer_id.to_string()
    }

    /// Mark a player disconnected — the roster entry (character included)
    /// is kept so a reconnect under the same name reattaches.
    pub fn mark_disconnected(&mut self, peer_id: &str) {
        let doc = &mut self.doc;
        let players = ensure_map(doc, &ROOT, "players");
        if let Some(p) = child(doc, &players, peer_id) {
            let _ = doc.put(&p, "connected", false);
        }
    }

    /// The numeric result of the most recent roll, if any (in-memory
    /// only; consumed by the host to ack secret GM rolls).
    pub fn take_last_roll(&mut self) -> Option<RollRecord> {
        self.last_roll.take()
    }

    /// JSON mirror of the doc (TS `SessionSnapshot`). Missing keys
    /// decode with defaults (v1 forward-compat).
    pub fn snapshot(&self) -> Result<SessionSnapshot> {
        let doc = &self.doc;
        let mut out = SessionSnapshot::default();

        if let Some(meta) = child(doc, &ROOT, "meta") {
            out.meta.id = get_str(doc, &meta, "id").unwrap_or_default();
            out.meta.name = get_str(doc, &meta, "name").unwrap_or_default();
            out.meta.gm_id = get_str(doc, &meta, "gm_id").unwrap_or_default();
            out.meta.created_at = get_i64(doc, &meta, "created_at").unwrap_or(0);
            out.meta.adventure_slug = match get_str(doc, &meta, "adventure_slug") {
                Some(s) if !s.is_empty() => Some(s),
                _ => None,
            };
        }

        if let Some(players) = child(doc, &ROOT, "players") {
            for key in keys(doc, &players) {
                let Some(p) = child(doc, &players, &key) else { continue };
                let character = child(doc, &p, "character").map(|c| read_sheet(doc, &c));
                out.players.insert(
                    key,
                    PlayerInfo {
                        name: get_str(doc, &p, "name").unwrap_or_default(),
                        character,
                        color: get_str(doc, &p, "color").unwrap_or_default(),
                        connected: get_bool(doc, &p, "connected").unwrap_or(true),
                    },
                );
            }
        }

        if let Some(chat) = child(doc, &ROOT, "chat") {
            for idx in 0..doc.length(&chat) {
                let Some((Value::Object(_), entry)) = doc.get(&chat, idx)? else { continue };
                let kind = match get_str(doc, &entry, "kind").as_deref() {
                    Some("roll") => ChatKind::Roll,
                    Some("system") => ChatKind::System,
                    _ => ChatKind::Chat,
                };
                let roll = child(doc, &entry, "roll").map(|r| RollRecord {
                    expr: get_str(doc, &r, "expr").unwrap_or_default(),
                    total: get_i64(doc, &r, "total").unwrap_or(0),
                    rolls: read_u32_list(doc, &r, "rolls"),
                    kept: read_u32_list(doc, &r, "kept"),
                });
                out.chat.push(ChatEntry {
                    ts: get_i64(doc, &entry, "ts").unwrap_or(0),
                    from: get_str(doc, &entry, "from").unwrap_or_default(),
                    kind,
                    text: get_str(doc, &entry, "text").unwrap_or_default(),
                    roll,
                });
            }
        }

        if let Some(init) = child(doc, &ROOT, "init") {
            out.init.active_idx = get_i64(doc, &init, "active_idx").unwrap_or(0);
            out.init.round = get_i64(doc, &init, "round").unwrap_or(0);
            if let Some(order) = child(doc, &init, "order") {
                out.init.order = read_str_list(doc, &order);
            }
        }

        if let Some(scene) = child(doc, &ROOT, "scene") {
            if let Some(grid) = child(doc, &scene, "grid") {
                out.scene.grid = Grid {
                    w: get_i64(doc, &grid, "w").unwrap_or(24),
                    h: get_i64(doc, &grid, "h").unwrap_or(24),
                    size: get_i64(doc, &grid, "size").unwrap_or(48),
                };
            }
            out.scene.background_entity_id =
                match get_str(doc, &scene, "background_entity_id") {
                    Some(s) if !s.is_empty() => Some(s),
                    _ => None,
                };
            if let Some(tokens) = child(doc, &scene, "tokens") {
                for key in keys(doc, &tokens) {
                    let Some(t) = child(doc, &tokens, &key) else { continue };
                    out.scene.tokens.insert(key, read_token(doc, &t));
                }
            }
        }

        if let Some(handouts) = child(doc, &ROOT, "handouts") {
            for key in keys(doc, &handouts) {
                let Some(h) = child(doc, &handouts, &key) else { continue };
                out.handouts.insert(
                    key,
                    HandoutState {
                        entity_id: get_str(doc, &h, "entity_id").unwrap_or_default(),
                        visible_to: get_str(doc, &h, "visible_to").unwrap_or("gm".into()),
                    },
                );
            }
        }

        Ok(out)
    }
}

impl SessionDoc {
    /// Run a closure with mutable access to the raw document.
    pub fn with_doc<R>(&mut self, f: impl FnOnce(&mut AutoCommit) -> R) -> R {
        f(&mut self.doc)
    }
}

// === SharedDoc: the Clone-able, thread-safe handle used by the service,
// the LAN bridge, and the iroh sync pump ==============================

#[derive(Clone)]
pub struct SharedDoc(Arc<Mutex<SessionDoc>>);

impl SharedDoc {
    pub fn new(doc: SessionDoc) -> Self {
        SharedDoc(Arc::new(Mutex::new(doc)))
    }

    /// Run a closure under the doc lock.
    pub fn with<R>(&self, f: impl FnOnce(&mut SessionDoc) -> R) -> R {
        let mut guard = self.0.lock().expect("session doc lock");
        f(&mut guard)
    }

    pub fn session_id(&self) -> String {
        self.with(|d| d.session_id().to_string())
    }

    /// Generate one sync message for the peer represented by `state`.
    pub fn generate_sync_message(&self, state: &mut sync::State) -> Option<sync::Message> {
        self.with(|d| d.doc.sync().generate_sync_message(state))
    }

    /// Apply a received sync message; if the doc changed, derive
    /// PatchEvents by snapshot diff and broadcast them.
    pub fn receive_sync_message(
        &self,
        state: &mut sync::State,
        message: sync::Message,
        events: &broadcast::Sender<SessionEvent>,
    ) -> Result<()> {
        let derived: Vec<SessionPatchEvent> = {
            let mut guard = self.0.lock().expect("session doc lock");
            let pre = guard.snapshot()?;
            let changed_heads = {
                let doc = &mut guard.doc;
                let before = doc.get_heads();
                doc.sync().receive_sync_message(state, message)
                    .context("receive sync message")?;
                let after = doc.get_heads();
                before != after
            };
            if !changed_heads {
                return Ok(());
            }
            let post = guard.snapshot()?;
            diff_snapshots(&guard.session_id, &pre, &post)
        };
        for event in derived {
            let _ = events.send(SessionEvent::Patch(event));
        }
        Ok(())
    }
}

// === Private helpers =====================================================

fn child(doc: &AutoCommit, parent: &ObjId, key: &str) -> Option<ObjId> {
    match doc.get(parent, key) {
        Ok(Some((Value::Object(_), id))) => Some(id),
        _ => None,
    }
}

fn ensure_map(doc: &mut AutoCommit, parent: &ObjId, key: &str) -> ObjId {
    match child(doc, parent, key) {
        Some(id) => id,
        None => doc
            .put_object(parent, key, ObjType::Map)
            .expect("create map"),
    }
}

fn ensure_list(doc: &mut AutoCommit, parent: &ObjId, key: &str) -> ObjId {
    match child(doc, parent, key) {
        Some(id) => id,
        None => doc
            .put_object(parent, key, ObjType::List)
            .expect("create list"),
    }
}

fn key_exists(doc: &AutoCommit, parent: &ObjId, key: &str) -> bool {
    matches!(doc.get(parent, key), Ok(Some(_)))
}

fn get_str(doc: &AutoCommit, obj: &ObjId, key: &str) -> Option<String> {
    match doc.get(obj, key) {
        Ok(Some((v, _))) => v.as_str().map(str::to_string),
        _ => None,
    }
}

fn get_bool(doc: &AutoCommit, obj: &ObjId, key: &str) -> Option<bool> {
    match doc.get(obj, key) {
        Ok(Some((v, _))) => v.as_bool(),
        _ => None,
    }
}

fn get_i64(doc: &AutoCommit, obj: &ObjId, key: &str) -> Option<i64> {
    match doc.get(obj, key) {
        Ok(Some((v, _))) => v.as_i64(),
        _ => None,
    }
}

fn keys(doc: &AutoCommit, obj: &ObjId) -> Vec<String> {
    doc.keys(obj).map(|k| k.to_string()).collect()
}

fn read_str_list(doc: &AutoCommit, list: &ObjId) -> Vec<String> {
    let mut out = Vec::with_capacity(doc.length(list));
    for idx in 0..doc.length(list) {
        if let Ok(Some((v, _))) = doc.get(list, idx) {
            if let Some(s) = v.as_str() {
                out.push(s.to_string());
            }
        }
    }
    out
}

fn read_u32_list(doc: &AutoCommit, parent: &ObjId, key: &str) -> Vec<u32> {
    match child(doc, parent, key) {
        Some(list) => read_int_list(doc, &list),
        None => Vec::new(),
    }
}

fn read_int_list(doc: &AutoCommit, list: &ObjId) -> Vec<u32> {
    let mut out = Vec::with_capacity(doc.length(list));
    for idx in 0..doc.length(list) {
        if let Ok(Some((v, _))) = doc.get(list, idx) {
            if let Some(n) = v.as_i64() {
                out.push(n.clamp(0, u32::MAX as i64) as u32);
            }
        }
    }
    out
}

fn put_str_list(doc: &mut AutoCommit, parent: &ObjId, key: &str, values: &[String]) -> Result<()> {
    doc.delete(parent, key).ok();
    let list = doc.put_object(parent, key, ObjType::List)?;
    for v in values {
        doc.insert(&list, doc.length(&list), v.as_str())?;
    }
    Ok(())
}

fn put_sheet(doc: &mut AutoCommit, obj: &ObjId, sheet: &CharacterSheet) -> Result<()> {
    doc.put(obj, "level", sheet.level)?;
    doc.put(obj, "class", sheet.class.as_str())?;
    doc.put(obj, "hp", sheet.hp)?;
    doc.put(obj, "max_hp", sheet.max_hp)?;
    doc.put(obj, "ac", sheet.ac)?;
    put_str_list(doc, obj, "conditions", &sheet.conditions)?;
    doc.put(obj, "notes", sheet.notes.as_str())?;
    Ok(())
}

fn put_sheet_patch(doc: &mut AutoCommit, obj: &ObjId, patch: &SheetPatch) -> Result<()> {
    if let Some(v) = patch.level {
        doc.put(obj, "level", v)?;
    }
    if let Some(v) = &patch.class {
        doc.put(obj, "class", v.as_str())?;
    }
    if let Some(v) = patch.hp {
        doc.put(obj, "hp", v)?;
    }
    if let Some(v) = patch.max_hp {
        doc.put(obj, "max_hp", v)?;
    }
    if let Some(v) = patch.ac {
        doc.put(obj, "ac", v)?;
    }
    if let Some(v) = &patch.conditions {
        put_str_list(doc, obj, "conditions", v)?;
    }
    if let Some(v) = &patch.notes {
        doc.put(obj, "notes", v.as_str())?;
    }
    Ok(())
}

fn read_sheet(doc: &AutoCommit, obj: &ObjId) -> CharacterSheet {
    CharacterSheet {
        level: get_i64(doc, obj, "level").unwrap_or(0),
        class: get_str(doc, obj, "class").unwrap_or_default(),
        hp: get_i64(doc, obj, "hp").unwrap_or(0),
        max_hp: get_i64(doc, obj, "max_hp").unwrap_or(0),
        ac: get_i64(doc, obj, "ac").unwrap_or(0),
        conditions: match child(doc, obj, "conditions") {
            Some(list) => read_str_list(doc, &list),
            None => Vec::new(),
        },
        notes: get_str(doc, obj, "notes").unwrap_or_default(),
    }
}

fn put_token(doc: &mut AutoCommit, obj: &ObjId, token: &Token) -> Result<()> {
    doc.put(obj, "token_id", token.token_id.as_str())?;
    doc.put(obj, "owner", token.owner.as_str())?;
    doc.put(obj, "entity_id", token.entity_id.clone().unwrap_or_default().as_str())?;
    doc.put(obj, "pc", token.pc.clone().unwrap_or_default().as_str())?;
    doc.put(obj, "x", token.x)?;
    doc.put(obj, "y", token.y)?;
    doc.put(obj, "label", token.label.as_str())?;
    Ok(())
}

fn read_token(doc: &AutoCommit, obj: &ObjId) -> Token {
    let opt = |key: &str| match get_str(doc, obj, key) {
        Some(s) if !s.is_empty() => Some(s),
        _ => None,
    };
    Token {
        token_id: get_str(doc, obj, "token_id").unwrap_or_default(),
        owner: get_str(doc, obj, "owner").unwrap_or_default(),
        entity_id: opt("entity_id"),
        pc: opt("pc"),
        x: match doc.get(obj, "x") {
            Ok(Some((v, _))) => v.as_f64().unwrap_or(0.0),
            _ => 0.0,
        },
        y: match doc.get(obj, "y") {
            Ok(Some((v, _))) => v.as_f64().unwrap_or(0.0),
            _ => 0.0,
        },
        label: get_str(doc, obj, "label").unwrap_or_default(),
    }
}

fn append_chat(doc: &mut AutoCommit, entry: ChatEntry) -> Result<()> {
    let chat = ensure_list(doc, &ROOT, "chat");
    let e = doc.insert_object(&chat, doc.length(&chat), ObjType::Map)?;
    doc.put(&e, "ts", entry.ts)?;
    doc.put(&e, "from", entry.from.as_str())?;
    doc.put(
        &e,
        "kind",
        match entry.kind {
            ChatKind::Chat => "chat",
            ChatKind::Roll => "roll",
            ChatKind::System => "system",
        },
    )?;
    doc.put(&e, "text", entry.text.as_str())?;
    if let Some(roll) = &entry.roll {
        doc.delete(&e, "roll").ok();
        let r = doc.put_object(&e, "roll", ObjType::Map)?;
        doc.put(&r, "expr", roll.expr.as_str())?;
        doc.put(&r, "total", roll.total)?;
        let rolls = doc.put_object(&r, "rolls", ObjType::List)?;
        for v in &roll.rolls {
            doc.insert(&rolls, doc.length(&rolls), *v as i64)?;
        }
        let kept = doc.put_object(&r, "kept", ObjType::List)?;
        for v in &roll.kept {
            doc.insert(&kept, doc.length(&kept), *v as i64)?;
        }
    }
    Ok(())
}
