# Phase 10 — Live Table · FROZEN CONTRACT

Baseline: `docs/phase10/PLAN.md`. This contract is binding on BUILDER-RUST and
BUILDER-WEB. PLAN wins on goals; this document wins on mechanics. Deviation requires
updating THIS file first.

## 1. Corrections to PLAN.md (vs. actual code)

1. **iroh version.** PLAN assumed ~0.9x API. Current is **iroh 1.3** (Node→Endpoint
   rename landed). Use `iroh::Endpoint`, `EndpointAddr`, `endpoint::presets::N0`;
   tickets live in the separate **`iroh-tickets`** crate (`endpoint::EndpointTicket`,
   `Ticket` trait, base32 strings). See §3.
2. **WS bridge dep.** PLAN: "axum WS bridge". Resolution: **tokio-tungstenite on
   `tokio::net::TcpListener`, no axum.** PLAN's dep budget allows either; tungstenite
   alone is far lighter. `lan.rs` speaks plain RFC-6455 with JSON text frames.
3. **Session file location.** PLAN: "campaign folder (`sessions/<id>.automerge`)".
   Actual campaign dirs live at `<vault>/campaigns/<slug>/` (vault_write.rs).
   Resolution: `<vault>/campaigns/<campaign_id>/sessions/<session_id>.automerge`
   (create dir on host; write debounced every 2 s after changes + on `EndSession`).
4. **Feature defaults.** PLAN: `p2p`, `lan` default-ON. Kept. Resolution: default
   `cargo test` compiles both but MUST stay green offline — the iroh test is
   loopback-only with an env kill-switch (§6). CI lean path: `cargo check --no-default-features`.
5. **Existing LLM tools confirmed.** `TOOL_IDS` already has exactly the 5 PLAN lists
   (`lookup_alias`, `lookup_miracle`, `search`, `validate_statblock`, `xp_budget`).
   Phase 10 adds **`roll_dice` only** and MUST NOT change `run_agent`'s signature —
   phase6_d tests call it directly. Add `run_agent_with_session(...)` (below);
   `dispatch_tool` keeps its signature; a new `dispatch_tool_session` handles
   `roll_dice` and delegates everything else to `dispatch_tool`.
6. **Event naming.** Existing convention (commands.rs `llm:token`): kebab-prefix
   channel, payload = `session_id` field + `#[serde(flatten)]` enum tagged
   `#[serde(tag = "kind", rename_all = "snake_case")]`. Session events follow it
   exactly: channels **`session:patch`**, **`session:presence`**; all enums in this
   phase use `tag="kind", snake_case`. No capability change needed (`core:default`
   already permits backend emit, same as `llm:token`).
7. **Adventure content location.** PLAN assigns `data/content/…/adventures/` to the
   frontend agent, but that tree is compiled into the binary via `include_dir!`
   (content.rs) and loaded by Rust. Resolution: files live at
   `src-tauri/data/content/lewisian/adventures/`; **ownership: BUILDER-WEB** (content
   authoring), loader + tests: BUILDER-RUST. See §7.
8. **Content-pack frontmatter.** Existing packs use `id/title/type/lens/
   license_provenance` + free-form `sources: […]` (citation.ts parses them), NOT a
   `pack.toml` `sources` key per-file. Adventure frontmatter uses the per-file keys;
   unknown frontmatter keys survive via the catch-all extras mapping (content.rs).
9. **IPA arg casing.** Tauri converts camelCase JS args → snake_case Rust params
   (see `xp_budget`/`{ partySize }`). All new bindings follow that convention.
10. **Dice randomness — no `rand` dep.** PLAN's dep budget excludes `rand`.
    `dice.rs` draws entropy from `uuid::Uuid::new_v4()` bytes with rejection
    sampling (avoids modulo bias). uuid is already a direct dep with `v4`.

## 2. Rust module layout (`src-tauri/src/`)

Tokio runtime: none owned by us — Tauri's async runtime already backs
`tokio::spawn` (see llm_chat). Services are spawned from the `session_host` /
`session_join` commands with `tokio::spawn`; unit tests use
`#[tokio::test(flavor = "multi_thread")]`.

```
src-tauri/src/
  dice.rs          pure dice parser+roller. #[derive] no deps beyond uuid.
     pub struct RollSpec { pub count: u32, pub sides: u32, pub modifier: i32, pub keep: Option<u32> }
     pub struct RollOutcome { pub expr: String, pub total: i64, pub rolls: Vec<u32>, pub kept: Vec<u32> }
     pub fn parse(expr: &str) -> Result<RollSpec, DiceError>;      // "2d6+3"[, "4d6k3"]; ≤100 dice, sides 2..=1000
     pub fn roll(spec: &RollSpec) -> RollOutcome;                  // rejection-sampled from Uuid bytes
  session/mod.rs   schema mirror types (TS §4), Intent enum, PatchEvent enum, Role, authority check.
     pub enum Role { Gm, Player }                                  // serde snake_case
     pub fn authorize(role: Role, actor_peer: &str, intent: &Intent) -> Result<(), IntentError>;
  session/doc.rs   automerge wrapper (automerge = "0.12", AutoCommit API).
     pub struct SessionDoc { /* AutoCommit */ }
     pub fn new_session(meta: SessionMeta) -> Self;
     pub fn load(bytes: &[u8]) -> Result<Self>;
     pub fn save(&mut self) -> Vec<u8>;
     pub fn apply_intent(&mut self, actor: &str, role: Role, intent: &Intent) -> Result<Vec<PatchEvent>>;
     pub fn snapshot(&self) -> Result<SessionSnapshot>;            // JSON mirror of §4
  session/service.rs  host/peer service (owns SessionDoc + broadcast).
     pub struct SessionHandle { pub session_id: String, pub role: Role, pub peer_id: String, /* … */ }
     pub struct SessionRegistry(Mutex<Option<ActiveSession>>);     // ONE active session per process
     pub async fn host(vault: &Path, campaign: &str, name: &str, adventure: Option<&str>) -> Result<SessionHandle>;
     pub async fn join_lan_stub(&self) -> ! /* n/a — mobile joins via WS, no IPC */;
     #[cfg(feature="p2p")] pub async fn join_peer(ticket: &str, name: &str) -> Result<SessionHandle>;
     pub async fn intent(&self, intent: Intent) -> Result<()>;     // authorize → apply → persist → broadcast: PatchEvents via SessionBroadcaster
     pub fn snapshot_now(&self) -> Result<SessionSnapshot>;
  lan.rs           #[cfg(feature="lan")] WS bridge.
     pub const LAN_PORT: u16 = 4780;
     pub async fn serve(listener: TcpListener, svc: Arc<SessionService>) -> Result<()>;  // §4.3 frames
     pub fn advertised_addr() -> Result<String>;                   // "ws://<lan-ip>:4780/<session_id>"
  net.rs           #[cfg(feature="p2p")] iroh sync pump. ALPN: b"pf2e-table/1".
     pub const ALPN: &[u8] = b"pf2e-table/1";
     pub async fn host_endpoint(doc: SharedDoc) -> Result<String>; // returns EndpointTicket string
     pub async fn connect_and_sync(ticket: &str, doc: SharedDoc) -> Result<()>;
  adventure.rs     adventure pack parsing/validation.
     pub struct AdventureDef { pub id, pub title, pub scenes: Vec<Scene>, pub handouts: Vec<Handout>, pub npcs: Vec<NpcRef> }
     pub fn parse_adventure(fm: &serde_yaml::Mapping, body: &str, dir: &Dir) -> Result<AdventureDef>;
     pub fn validate_adventure(def: &AdventureDef) -> Vec<String>; // incl. rules::validate_statblock per NPC
  commands.rs      += session_host, session_join_peer(p2p), session_intent,
                     session_snapshot, session_leave, session_load(campaign_id)
  lib.rs           += mod dice; pub mod session; mod adventure; #[cfg]s lan, net;
                     manage Arc<SessionRegistry>; 6 new entries in generate_handler![]
  llm_tools.rs     += "roll_dice" in TOOL_IDS + spec {expr, secret?=false};
                     pub fn run_agent_with_session(db, registry, messages, system, cache_system,
                       temperature, max_tokens, session: Option<Arc<SessionHandle>>)
                       -> mpsc::UnboundedReceiver<Result<AgentEvent>>
                     dispatch_tool_session(call, db, registry, session): "roll_dice" requires
                     session + applies Intent::RollDice (actor = "gm" peer; GM-only tool),
                     returns {expr,total,rolls}; else delegates to dispatch_tool.
```

### Cargo.toml additions (exact)

```toml
[features]
default = ["lan", "p2p"]
lan = ["dep:tokio-tungstenite"]
p2p = ["dep:iroh", "dep:iroh-tickets"]

[dependencies]
automerge = "0.12"
tokio-tungstenite = { version = "0.30", optional = true }
iroh = { version = "1.3", optional = true }
iroh-tickets = { version = "0.5", optional = true }   # verify ≥ the 1.x-compatible release; pin in Cargo.lock
```
Nothing else may be added. `iroh` transitive deps must not be named in our Cargo.toml.

### iroh 1.3 calls (freeze; PLAN's 0.9x names superseded)

```rust
// host
let ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0)
    .alpns(vec![ALPN.to_vec()]).bind().await?;
while let Some(incoming) = ep.accept().await { let conn = incoming.await?; … }
// join
let addr: iroh::EndpointAddr = iroh_tickets::endpoint::EndpointTicket::deserialize(ticket_str)?.endpoint_addr().clone();
let conn = iroh::Endpoint::bind(iroh::endpoint::presets::N0).await?.connect(addr, ALPN).await?;
let (send, recv) = conn.open_bi().await?;           // sync pump; peer uses accept_bi()
```
Sync framing on the bi-stream: each frame = 4-byte LE length + `automerge::sync::Message::encode()`
bytes; receive side feeds `SessionDoc::receive_sync_message`, generate with
`generate_sync_message`; loop until both idle ≥500 ms, then stay open for change-triggered
re-sync. Loopback tests: bind two in-process endpoints, disable relays
(`RelayMode::Disabled` via builder) and connect via `EndpointAddr::new(endpoint_id)`
+ loopback addrs from `ep.addr()`.

## 3. TypeScript contract (`src/lib/`)

### ipc.ts additions (BUILDER-WEB authors; names frozen)

```ts
// Phase 10 — Live Table. Mirrors src-tauri/src/session/*.
export type Role = "gm" | "player";

export type Intent =
  | { kind: "create_character"; name: string; color: string; character: CharacterSheet }
  | { kind: "update_sheet"; patch: Partial<CharacterSheet> }            // self only
  | { kind: "roll_dice"; expr: string; secret?: boolean }               // GM may set secret
  | { kind: "chat"; text: string }
  | { kind: "reveal_handout"; handout_id: string }                      // gm
  | { kind: "set_initiative"; order: string[] }                         // gm
  | { kind: "advance_initiative" }                                      // gm
  | { kind: "move_token"; token_id: string; x: number; y: number }      // gm or token owner
  | { kind: "add_token"; token: Token }                                 // gm
  | { kind: "remove_token"; token_id: string }                          // gm
  | { kind: "set_scene"; grid: Grid; background_entity_id?: string|null }// gm
  | { kind: "load_adventure"; slug: string }                            // gm
  | { kind: "end_session" };                                            // gm

export interface CharacterSheet { level: number; class: string; hp: number; max_hp: number;
  ac: number; conditions: string[]; notes: string }
export interface Grid { w: number; h: number; size: number }
export interface Token { token_id: string; owner: string; entity_id: string|null; pc: string|null;
  x: number; y: number; label: string }
export interface RollRecord { expr: string; total: number; rolls: number[]; kept: number[] }
export interface ChatEntry { ts: number; from: string; kind: "chat"|"roll"|"system";
  text: string; roll: RollRecord|null }

export interface SessionInfo { session_id: string; peer_id: string; role: Role;
  lan_url: string|null; p2p_ticket: string|null; name: string }

export const sessionHost = (campaignId: string, name: string, adventureSlug: string|null=null) =>
  invoke<SessionInfo>("session_host", { campaignId, name, adventureSlug });
export const sessionJoinPeer = (ticket: string, name: string) =>
  invoke<SessionInfo>("session_join_peer", { ticket, name });
export const sessionLoad = (campaignId: string, sessionId: string) =>
  invoke<SessionInfo>("session_load", { campaignId, sessionId });
export const sessionIntent = (intent: Intent) => invoke<void>("session_intent", { intent });
export const sessionLeave = () => invoke<void>("session_leave");
```

### Session state mirror + PatchEvent (Tauri events `session:patch`, `session:presence`)

```ts
export interface SessionSnapshot {
  meta: { id: string; name: string; gm_id: string; created_at: number; adventure_slug: string|null };
  players: Record<string, { name: string; character: CharacterSheet|null; color: string }>;
  chat: ChatEntry[];
  init: { order: string[]; active_idx: number; round: number };
  scene: { grid: Grid; background_entity_id: string|null; tokens: Record<string, Token> };
  handouts: Record<string, { entity_id: string; visible_to: "all"|"gm" }>;
}

export type PatchEvent =
  | { session_id: string; kind: "snapshot"; snapshot: SessionSnapshot }         // sent on join/load
  | { session_id: string; kind: "chat_appended"; entry: ChatEntry }
  | { session_id: string; kind: "player_upserted"; peer_id: string;
      player: { name: string; character: CharacterSheet|null; color: string } }
  | { session_id: string; kind: "player_removed"; peer_id: string }
  | { session_id: string; kind: "initiative"; order: string[]; active_idx: number; round: number }
  | { session_id: string; kind: "token_upserted"; token: Token }
  | { session_id: string; kind: "token_removed"; token_id: string }
  | { session_id: string; kind: "scene_set"; scene: SessionSnapshot["scene"] }
  | { session_id: string; kind: "handout_revealed"; handout_id: string }
  | { session_id: string; kind: "adventure_loaded"; slug: string }
  | { session_id: string; kind: "session_ended" }
  | { session_id: string; kind: "error"; message: string };                     // authority rejection etc.

export type PresenceEvent =
  | { session_id: string; kind: "peer_joined"; peer_id: string; name: string; via: "ipc"|"lan"|"p2p" }
  | { session_id: string; kind: "peer_left"; peer_id: string };
```

Rust enums are `#[serde(tag="kind", rename_all="snake_case")]` with
`session_id` flattened in, exactly like `LlmTokenEvent`. The UI reducer applies
PatchEvents in arrival order; `snapshot` replaces state wholesale.

### transport.ts — full interface (BUILDER-WEB; the ONLY play-UI seam)

```ts
// src/lib/transport.ts — one surface, two backends. Play UI imports ONLY this.
import type { Intent, PatchEvent, PresenceEvent } from "$lib/ipc";

export interface Transport {
  readonly mode: "ipc" | "ws";
  connect(): Promise<void>;                       // ipc: subscribes; ws: open + hello handshake
  send(intent: Intent): Promise<void>;            // ws authority rejections surface via "error" frame
  onPatch(cb: (p: PatchEvent) => void): () => void;
  onPresence(cb: (p: PresenceEvent) => void): () => void;
  close(): Promise<void>;
}
/** Desktop host/peer path: IPC + listen("session:patch" / "session:presence"). */
export function ipcTransport(): Transport;
/** Mobile thin client: WebSocket to host lan_url, frames per §4.3. */
export function wsTransport(lanUrl: string, name: string): Transport;
/** Test double for vitest; records intents, feeds patches from a queue. */
export function mockTransport(): Transport & { sent: Intent[]; emit(p: PatchEvent): void };
```
Backend selection (freeze): `window.__TAURI_INTERNALS__ !== undefined` → `ipcTransport()`;
otherwise require `?host=ws://…/<session_id>` query param → `wsTransport(host, name)`.
New `src/lib/session.svelte.ts` store (`$state`, mirrors llm.svelte.ts shape) holds
`{ info: SessionInfo|null, snapshot: SessionSnapshot|null, presence: string[] }` with a
single `applyPatch(p: PatchEvent)` reducer — ALL play-UI reads go through it.

## 4. Session doc schema v1 (automerge 0.12), sync, LAN protocol, authority

### 4.1 Doc schema — the TS `SessionSnapshot` above IS the v1 schema

Top-level automerge keys: `meta` (map, incl. `schema_version: 1`), `players` (map by
peer_id), `chat` (list, append-only), `init` (map), `scene` (map incl. `tokens` map),
`handouts` (map). Write domains: an author writes only its own `players/<self>`
subtree and appends to `chat`; all other keys are GM-writes-only (enforced by
`authorize` + by convention on CRDT peers: peers never issue GM intents — see table).
`created_at`/`ts` are unix millis (i64). Decoders tolerate missing keys with defaults
(v1 forward-compat); never delete/rename keys without bumping `meta.schema_version`.

### 4.2 P2P sync (feature `p2p`, iroh 1.3)

`automerge::sync` over one long-lived bi-stream per peer pair; frame =
`u32_le_len || sync::Message::encode()`. Host accepts on ALPN `b"pf2e-table/1"`;
joiner opens the stream FIRST (iroh: receiver's `accept_bi` blocks until sender writes).
Per-peer `sync::State`; after each receive, regenerate until `generate_sync_message`
yields None, then idle; every successful local `apply_intent` (and each observed
remote merge) triggers a new pump on all open streams. PatchEvents after a merge are
derived by diffing pre/post `snapshot()` (cheap at v1 doc sizes).

### 4.3 LAN WS protocol (feature `lan`, JSON text frames, default port 4780)

Path `/ws/<session_id>`. All frames are flat JSON objects with a `type` key:

```
client→host {"type":"hello","session_id":"…","name":"<player>"}
host→client {"type":"welcome","session_id":"…","peer_id":"…","role":"player"}
host→client {"type":"patch","session_id":"…","kind":"snapshot","snapshot":{…}}   // right after welcome
client→host {"type":"intent","intent":<Intent §3>}
host→all    {"type":"patch","session_id":"…",<PatchEvent fields>}
host→all    {"type":"presence","session_id":"…","kind":"peer_joined","peer_id":"…","name":"…","via":"lan"}
host→client {"type":"error","message":"…","rejected_intent":<Intent>?}
```
Wire PatchEvent/PresenceEvent payloads are byte-identical to the Tauri event payloads
(same serde enums re-serialized). Unauthorized/malformed intents → `error` to the
sender only, no broadcast. Socket close → `peer_left`. No app-level ping (tungstenite
keeps protocol pings). Thin clients never mutate locally: their reducer path is
patch-driven, identical to IPC.

### 4.4 Authority table (`session::authorize`, re-checked in lan.rs)

| Intent | gm | player (self) |
|---|---|---|
| create_character | ✓ | ✓ own peer_id only |
| update_sheet | ✓ any pc | ✓ own character only |
| roll_dice (`secret:true`) | ✓ | ✓ — but `secret` stripped unless gm |
| chat | ✓ | ✓ |
| move_token | ✓ | ✓ only token.owner == self |
| reveal_handout · set_initiative · advance_initiative · add_token · remove_token · set_scene · load_adventure · end_session | ✓ | ✗ → `IntentError::Forbidden` |

LLM `roll_dice` tool always executes as the GM peer on the host; it is rejected on
non-host processes (no session handle there). `IntentError` serializes
`{"kind":"forbidden"|"invalid","message":…}`.

## 5. Adventure pack format (WEB authors content; RUST owns loader)

```
src-tauri/data/content/lewisian/adventures/<adventure-stem>/                    # one dir per adventure (as-built)
src-tauri/data/content/lewisian/adventures/<adventure-stem>/<stem>.md           # adventure entity
src-tauri/data/content/lewisian/adventures/<adventure-stem>/<stem>.<npc>.statblock.json  # NPC sidecars
# (amended post-review: subdir layout replaces the original flat layout; sidecars are
#  resolved within the adventure's own dir, anchored by the adventure stem)
```
Frontmatter — existing pack keys + `scenes`/`handouts`/`npcs` (content.rs extras
pass-through preserves them into `entities.frontmatter`):
```yaml
---
id: lewisian:adventure-widows-lamp
title: "The Widow's Lamp"
type: adventure
lens: lewisian
license_provenance: homebrew
sources:
  - "original work — github.com/average-gary/pf2e-companion"
  - "wiki:pf2e-biblical-reskin/widows-lamp-scenario"
level_range: [1, 2]
scenes:
  - { id: scene-1, title: "…", read_aloud: "…" }
  - { id: scene-2, title: "…", encounter: { difficulty: moderate, npcs: [debt-collector] } }
  # 3 scenes total; encounter optional
handouts:
  - { id: letter, title: "…", visible_to: all, body: "…" }        # visible_to: all|gm, 2 total
npcs: [lamplighter, debt-collector, widow-mara]                   # 3 sidecar slugs
---
Body = GM run-notes markdown (free headings).
```
Sidecar `widows-lamp.<npc>.statblock.json` follows `deities/yhwh.statblock.json`:
requires `name/level/type`, `license_provenance:"homebrew"`. Loading:
`adventure::parse_adventure` runs during `load_bundled_packs` for `type: adventure`;
`load_adventure` IPC path re-reads the parsed def from the entity's
frontmatter/statblock columns. Validation (phase10_adventure.rs): unique scene/handout
ids, each `npcs` slug has a sidecar that parses and passes
`rules::validate_statblock(...).valid`. Startup loader logs-and-skips on failure —
never panics. `Intent::LoadAdventure(slug)` sets `meta.adventure_slug`, populates
`scene.tokens` from encounter npcs, registers `handouts` (gm-visibility), and appends
a `kind:"system"` chat entry with scene-1 read-aloud.

## 6. Test plan (exact files)

### `src-tauri/tests/` (style of phase6_d.rs; inline fixtures)
- `phase10_dice.rs` — "2d6+3" parse/roll bounds; "4d6k3" keep rule; bad-expr errors;
  100-die cap enforced.
- `phase10_session.rs` — save/load round-trip byte-stable; authority matrix negative
  cases (cross-player `update_sheet`, unowned `move_token`, player `end_session`);
  `apply_intent` → PatchEvent mapping for every variant of §3.
- `phase10_sync.rs` — two `SessionDoc`s exchange raw `sync::Message`s in-process via
  loop; snapshots converge. Feature-independent, runs on `--no-default-features`.
- `phase10_adventure.rs` — bundled widows-lamp parses; asserts 3 scenes / 2 handouts /
  3 NPCs; every sidecar `validate_statblock` valid.
- `phase10_lan.rs` `#![cfg(feature = "lan")]` — `TcpListener` on 127.0.0.1:0,
  in-process tungstenite client: hello→welcome→snapshot; chat intent broadcast;
  wrong-authority intent → `error` frame, state unchanged.
- `phase10_table_demo.rs` `#![cfg(feature = "lan")]` — PLAN gate 4 end-to-end:
  host service + one WS client; `roll_dice` visible via host emit bus AND client patch;
  client's `update_sheet` targeting the GM's pc rejected with `error`.
- `phase10_p2p.rs` `#![cfg(all(feature = "p2p"))]` — iroh loopback convergence,
  `RelayMode::Disabled`, connect via loopback `EndpointAddr`. **Offline guard**:
  first line `if std::env::var("PF2E_NET_TESTS").as_deref() != Ok("1") { return; }` —
  default `cargo test` passes with zero network.

### `src/` (vitest node env; vite.config.js includes `src/**/*.test.ts`)
- `src/lib/transport.test.ts` — mockTransport records/emit/unsubscribe; wsTransport
  frame encoding vs. a fake-global `WebSocket`; hello handshake drives `welcome`.
- `src/lib/session.svelte.test.ts` — reducer: snapshot replace; chat ordering by ts;
  token_upsert merge; handout_revealed flips visibility; session_ended resets; unknown
  `kind` is ignored without throwing (v1 tolerance rule).

## 7. Ownership map (disjoint)

| Owner | Globs |
|---|---|
| BUILDER-RUST | `src-tauri/src/**`, `src-tauri/tests/**`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` |
| BUILDER-WEB | `src/**` (incl. `ipc.ts`, `transport.ts`, `session.svelte.ts`, new `/table` route + `+layout.svelte` tab), `src-tauri/data/content/**` (adventure files only), `README.md` (new Phase 10 section) |
| FROZEN — neither edits | `docs/phase10/PLAN.md`, `docs/phase10/CONTRACT.md` |

Cross-boundary needs are handled by amending THIS file, not by editing the other side.
WEB programs against §3 byte-for-byte; RUST implements exactly those names.

## 8. Guardrails

- Rust: iterate with `cargo check` / `cargo test` in `src-tauri/`; finish with
  `cargo test --all-features` + `cargo check --no-default-features`.
  **NEVER `pnpm tauri build`** (heavy, CI-only).
- Web: `pnpm test` + `pnpm check`. No new npm deps (`@tauri-apps/api` + DOM
  WebSocket suffice).
- Cargo deps: ONLY the §2 block rows (automerge, tokio-tungstenite, iroh, iroh-tickets).
  **Forbidden**: rand, axum, QR crates, tauri plugins.
- `llm_tools.rs` delta = EXACTLY ONE tool (`roll_dice`, host/GM-scoped) plus
  `run_agent_with_session`; zero changes to existing tool specs or `run_agent`
  signature — all phase ≤ 6.5 tests must pass unmodified.
- `Intent`, `PatchEvent`, `PresenceEvent`, WS frames: serde shapes are copied from
  this file only; tag = `kind`, snake_case, everywhere.
