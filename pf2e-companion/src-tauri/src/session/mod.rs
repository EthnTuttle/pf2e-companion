//! Live Table session types — the Rust mirror of CONTRACT § 3/§ 4.
//!
//! All serde shapes are copied from the frozen contract: enums use
//! `tag = "kind"`, `snake_case`; `session_id` is flattened into the
//! patch/presence wrappers exactly like `LlmTokenEvent`.

pub mod doc;
pub mod service;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

pub use doc::{SessionDoc, SessionMeta, SharedDoc};
pub use service::{
    ActiveSession, SessionHandle, SessionInfo, SessionRegistry, SessionService,
};

/// Peer role. Wire form: `"gm"` / `"player"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Gm,
    Player,
}

/// A player character sheet (TS `CharacterSheet`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CharacterSheet {
    pub level: i64,
    pub class: String,
    pub hp: i64,
    pub max_hp: i64,
    pub ac: i64,
    #[serde(default)]
    pub conditions: Vec<String>,
    #[serde(default)]
    pub notes: String,
}

/// Partial sheet update (TS `Partial<CharacterSheet>`). Absent fields are
/// left untouched.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SheetPatch {
    #[serde(default)]
    pub level: Option<i64>,
    #[serde(default)]
    pub class: Option<String>,
    #[serde(default)]
    pub hp: Option<i64>,
    #[serde(default)]
    pub max_hp: Option<i64>,
    #[serde(default)]
    pub ac: Option<i64>,
    #[serde(default)]
    pub conditions: Option<Vec<String>>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Battle grid (TS `Grid`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Grid {
    pub w: i64,
    pub h: i64,
    pub size: i64,
}

impl Default for Grid {
    fn default() -> Self {
        Grid { w: 24, h: 24, size: 48 }
    }
}

/// A scene token (TS `Token`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Token {
    pub token_id: String,
    pub owner: String,
    #[serde(default)]
    pub entity_id: Option<String>,
    #[serde(default)]
    pub pc: Option<String>,
    pub x: f64,
    pub y: f64,
    pub label: String,
}

/// One die-roll record (TS `RollRecord`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RollRecord {
    pub expr: String,
    pub total: i64,
    pub rolls: Vec<u32>,
    pub kept: Vec<u32>,
}

/// Chat-entry kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatKind {
    Chat,
    Roll,
    System,
}

/// Append-only chat/roll log entry (TS `ChatEntry`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatEntry {
    pub ts: i64,
    pub from: String,
    pub kind: ChatKind,
    pub text: String,
    #[serde(default)]
    pub roll: Option<RollRecord>,
    /// Monotonic per-session sequence (CONTRACT-10_5 § 3) for client-side
    /// dedupe of `chat_appended` patches. `None` only for pre-10.5
    /// entries in legacy saves.
    #[serde(default)]
    pub seq: Option<u64>,
}

/// Player-visible info (TS `players` map value). `connected` is additive
/// (v1.1): the roster keeps disconnected peers' entries — character
/// included — so a reconnect under the same name reattaches; absent =
/// connected (legacy payloads).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub name: String,
    #[serde(default)]
    pub character: Option<CharacterSheet>,
    pub color: String,
    #[serde(default = "default_connected")]
    pub connected: bool,
}

fn default_connected() -> bool {
    true
}

/// Initiative tracker state (TS `init`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InitiativeState {
    #[serde(default)]
    pub order: Vec<String>,
    #[serde(default)]
    pub active_idx: i64,
    #[serde(default)]
    pub round: i64,
}

/// Scene state (TS `scene`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneState {
    #[serde(default)]
    pub grid: Grid,
    #[serde(default)]
    pub background_entity_id: Option<String>,
    #[serde(default)]
    pub tokens: BTreeMap<String, Token>,
}

/// Handout registry entry (TS `handouts` map value).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandoutState {
    pub entity_id: String,
    /// `"all"` or `"gm"`.
    pub visible_to: String,
}

/// Session metadata (TS `SessionSnapshot.meta`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMetaSnapshot {
    pub id: String,
    pub name: String,
    pub gm_id: String,
    pub created_at: i64,
    #[serde(default)]
    pub adventure_slug: Option<String>,
}

/// The full v1 session state (TS `SessionSnapshot`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub meta: SessionMetaSnapshot,
    #[serde(default)]
    pub players: BTreeMap<String, PlayerInfo>,
    #[serde(default)]
    pub chat: Vec<ChatEntry>,
    #[serde(default)]
    pub init: InitiativeState,
    #[serde(default)]
    pub scene: SceneState,
    #[serde(default)]
    pub handouts: BTreeMap<String, HandoutState>,
}

/// Session intents — the ONLY way state mutates (TS `Intent`).
///
/// `update_sheet` carries an optional `peer_id` target: absent (or equal
/// to the actor) means "my own sheet". The TS type omits the field, so
/// frontend intents always target self; a malicious peer sending a
/// foreign `peer_id` is rejected by the authority check (§ 4.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Intent {
    CreateCharacter {
        name: String,
        color: String,
        character: CharacterSheet,
    },
    UpdateSheet {
        #[serde(default)]
        peer_id: Option<String>,
        patch: SheetPatch,
    },
    RollDice {
        expr: String,
        #[serde(default)]
        secret: bool,
    },
    Chat {
        text: String,
    },
    RevealHandout {
        handout_id: String,
    },
    SetInitiative {
        order: Vec<String>,
    },
    AdvanceInitiative,
    MoveToken {
        token_id: String,
        x: f64,
        y: f64,
    },
    AddToken {
        token: Token,
    },
    RemoveToken {
        token_id: String,
    },
    SetScene {
        grid: Grid,
        #[serde(default)]
        background_entity_id: Option<String>,
    },
    LoadAdventure {
        slug: String,
    },
    EndSession,
}

/// Authority rejection. Wire form: `{"kind":"forbidden"|"invalid","message":…}`.
#[derive(Debug, Clone, PartialEq, Error, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IntentError {
    #[error("forbidden: {message}")]
    Forbidden { message: String },
    #[error("invalid: {message}")]
    Invalid { message: String },
}

impl IntentError {
    pub fn forbidden(message: impl Into<String>) -> Self {
        IntentError::Forbidden { message: message.into() }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        IntentError::Invalid { message: message.into() }
    }
}

/// Static authority check (CONTRACT § 4.4). Doc-context checks — token
/// ownership, sheet target — are re-checked in `SessionDoc::apply_intent`
/// and in `lan.rs`, which see the live document.
pub fn authorize(role: Role, actor_peer: &str, intent: &Intent) -> Result<(), IntentError> {
    let _ = actor_peer;
    if role == Role::Gm {
        return Ok(());
    }
    match intent {
        Intent::CreateCharacter { .. }
        | Intent::RollDice { .. }
        | Intent::Chat { .. } => Ok(()),
        Intent::UpdateSheet { peer_id, .. } => match peer_id {
            None => Ok(()),
            Some(target) if target == actor_peer => Ok(()),
            Some(target) => Err(IntentError::forbidden(format!(
                "players may only update their own sheet (targeted {target})"
            ))),
        },
        Intent::MoveToken { .. } => Ok(()), // ownership checked against the doc
        Intent::RevealHandout { .. }
        | Intent::SetInitiative { .. }
        | Intent::AdvanceInitiative
        | Intent::AddToken { .. }
        | Intent::RemoveToken { .. }
        | Intent::SetScene { .. }
        | Intent::LoadAdventure { .. }
        | Intent::EndSession => Err(IntentError::forbidden(
            "this intent is reserved for the GM",
        )),
    }
}

/// PatchEvent payloads (TS `PatchEvent`, `session_id` excluded here — the
/// [`SessionPatchEvent`] wrapper flattens it in).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PatchEventPayload {
    Snapshot {
        snapshot: SessionSnapshot,
    },
    ChatAppended {
        entry: ChatEntry,
    },
    PlayerUpserted {
        peer_id: String,
        player: PlayerInfo,
    },
    PlayerRemoved {
        peer_id: String,
    },
    Initiative {
        order: Vec<String>,
        active_idx: i64,
        round: i64,
    },
    TokenUpserted {
        token: Token,
    },
    TokenRemoved {
        token_id: String,
    },
    SceneSet {
        scene: SceneState,
    },
    HandoutRevealed {
        handout_id: String,
    },
    AdventureLoaded {
        slug: String,
    },
    SessionEnded,
    Error {
        message: String,
    },
}

/// `session:patch` event payload — `session_id` + flattened kind, exactly
/// like `LlmTokenEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionPatchEvent {
    pub session_id: String,
    #[serde(flatten)]
    pub payload: PatchEventPayload,
}

impl SessionPatchEvent {
    pub fn new(session_id: impl Into<String>, payload: PatchEventPayload) -> Self {
        SessionPatchEvent { session_id: session_id.into(), payload }
    }

    pub fn snapshot(session_id: impl Into<String>, snapshot: SessionSnapshot) -> Self {
        SessionPatchEvent::new(session_id, PatchEventPayload::Snapshot { snapshot })
    }
}

/// PresenceEvent payloads (TS `PresenceEvent`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PresenceEventPayload {
    PeerJoined {
        peer_id: String,
        name: String,
        via: String, // "ipc" | "lan" | "p2p"
    },
    PeerLeft {
        peer_id: String,
    },
}

/// `session:presence` event payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionPresenceEvent {
    pub session_id: String,
    #[serde(flatten)]
    pub payload: PresenceEventPayload,
}

/// Derive PatchEvents by diffing two snapshots (CONTRACT § 4.2 — used
/// after remote merges and around local applies). Events are returned in
/// arrival order; the UI reducer applies them sequentially.
pub fn diff_snapshots(session_id: &str, pre: &SessionSnapshot, post: &SessionSnapshot) -> Vec<SessionPatchEvent> {
    let mut events = Vec::new();
    let mk = |p: PatchEventPayload| SessionPatchEvent::new(session_id, p);

    // New chat entries (append-only log).
    if post.chat.len() > pre.chat.len() {
        for entry in &post.chat[pre.chat.len()..] {
            events.push(mk(PatchEventPayload::ChatAppended { entry: entry.clone() }));
        }
    }

    // Player upserts / removals.
    for (peer_id, player) in &post.players {
        if pre.players.get(peer_id) != Some(player) {
            events.push(mk(PatchEventPayload::PlayerUpserted {
                peer_id: peer_id.clone(),
                player: player.clone(),
            }));
        }
    }
    for peer_id in pre.players.keys() {
        if !post.players.contains_key(peer_id) {
            events.push(mk(PatchEventPayload::PlayerRemoved { peer_id: peer_id.clone() }));
        }
    }

    // Initiative.
    if pre.init != post.init {
        events.push(mk(PatchEventPayload::Initiative {
            order: post.init.order.clone(),
            active_idx: post.init.active_idx,
            round: post.init.round,
        }));
    }

    // Tokens + scene.
    let scene_changed = pre.scene.grid != post.scene.grid
        || pre.scene.background_entity_id != post.scene.background_entity_id;
    for (token_id, token) in &post.scene.tokens {
        if pre.scene.tokens.get(token_id) != Some(token) {
            events.push(mk(PatchEventPayload::TokenUpserted { token: token.clone() }));
        }
    }
    for token_id in pre.scene.tokens.keys() {
        if !post.scene.tokens.contains_key(token_id) {
            events.push(mk(PatchEventPayload::TokenRemoved { token_id: token_id.clone() }));
        }
    }
    if scene_changed {
        events.push(mk(PatchEventPayload::SceneSet { scene: post.scene.clone() }));
    }

    // Handouts: emit on registration and on reveal (gm → all).
    for (handout_id, handout) in &post.handouts {
        let was_hidden = matches!(
            pre.handouts.get(handout_id),
            Some(HandoutState { visible_to: v, .. }) if v == "gm"
        );
        if was_hidden && handout.visible_to == "all" {
            events.push(mk(PatchEventPayload::HandoutRevealed {
                handout_id: handout_id.clone(),
            }));
        }
    }

    // Adventure.
    if pre.meta.adventure_slug != post.meta.adventure_slug {
        if let Some(slug) = &post.meta.adventure_slug {
            events.push(mk(PatchEventPayload::AdventureLoaded { slug: slug.clone() }));
        }
    }

    events
}

impl Default for SessionSnapshot {
    fn default() -> Self {
        SessionSnapshot {
            meta: SessionMetaSnapshot {
                id: String::new(),
                name: String::new(),
                gm_id: String::new(),
                created_at: 0,
                adventure_slug: None,
            },
            players: BTreeMap::new(),
            chat: Vec::new(),
            init: InitiativeState::default(),
            scene: SceneState::default(),
            handouts: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> CharacterSheet {
        CharacterSheet {
            level: 1,
            class: "Fighter".into(),
            hp: 18,
            max_hp: 18,
            ac: 18,
            conditions: vec![],
            notes: String::new(),
        }
    }

    #[test]
    fn intent_serde_uses_kind_snake_case() {
        let json = serde_json::to_value(Intent::AdvanceInitiative).unwrap();
        assert_eq!(json, serde_json::json!({"kind": "advance_initiative"}));
        let json = serde_json::to_value(Intent::RollDice {
            expr: "2d6+3".into(),
            secret: false,
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({"kind": "roll_dice", "expr": "2d6+3", "secret": false})
        );
        // Round-trip a payload-heavy intent.
        let intent = Intent::CreateCharacter {
            name: "Ruth".into(),
            color: "#ff0000".into(),
            character: sheet(),
        };
        let rt: Intent = serde_json::from_value(serde_json::to_value(&intent).unwrap()).unwrap();
        assert_eq!(rt, intent);
    }

    #[test]
    fn intent_error_serializes_kind() {
        let e = IntentError::forbidden("nope");
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["kind"], "forbidden");
        assert_eq!(json["message"], "nope");
    }

    #[test]
    fn patch_event_flattens_session_id() {
        let ev = SessionPatchEvent::new(
            "s1",
            PatchEventPayload::PlayerRemoved { peer_id: "p1".into() },
        );
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["session_id"], "s1");
        assert_eq!(json["kind"], "player_removed");
        assert_eq!(json["peer_id"], "p1");
    }

    #[test]
    fn gm_can_do_everything_player_cannot_do_gm_intents() {
        let gm = Intent::EndSession;
        assert!(authorize(Role::Gm, "gm-peer", &gm).is_ok());
        assert!(authorize(Role::Player, "p1", &gm).is_err());
        assert!(authorize(Role::Player, "p1", &Intent::Chat { text: "hi".into() }).is_ok());
        // Cross-player sheet target is forbidden for players…
        assert!(authorize(
            Role::Player,
            "p1",
            &Intent::UpdateSheet { peer_id: Some("p2".into()), patch: Default::default() }
        )
        .is_err());
        // …but fine for the GM, and fine when implicit (self).
        assert!(authorize(
            Role::Gm,
            "gm",
            &Intent::UpdateSheet { peer_id: Some("p2".into()), patch: Default::default() }
        )
        .is_ok());
        assert!(authorize(
            Role::Player,
            "p1",
            &Intent::UpdateSheet { peer_id: None, patch: Default::default() }
        )
        .is_ok());
    }

    #[test]
    fn diff_detects_chat_and_players() {
        let meta = SessionMetaSnapshot {
            id: "s1".into(),
            name: "n".into(),
            gm_id: "g".into(),
            created_at: 0,
            adventure_slug: None,
        };
        let mut pre = SessionSnapshot { meta, ..Default::default() };
        pre.chat.push(ChatEntry {
            ts: 1,
            from: "g".into(),
            kind: ChatKind::Chat,
            text: "first".into(),
            roll: None,
            seq: None,
        });
        let mut post = pre.clone();
        post.chat.push(ChatEntry {
            ts: 2,
            from: "g".into(),
            kind: ChatKind::Chat,
            text: "second".into(),
            roll: None,
            seq: None,
        });
        post.players.insert(
            "p1".into(),
            PlayerInfo { name: "Ruth".into(), character: None, color: "#fff".into(), connected: true },
        );

        let events = diff_snapshots("s1", &pre, &post);
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0].payload,
            PatchEventPayload::ChatAppended { entry } if entry.text == "second"
        ));
        assert!(matches!(
            &events[1].payload,
            PatchEventPayload::PlayerUpserted { peer_id, .. } if peer_id == "p1"
        ));
    }
}
