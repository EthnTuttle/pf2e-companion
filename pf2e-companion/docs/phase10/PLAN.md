# Phase 10 — Live Table (VTT) Plan

**Goal**: turn the reference companion into a usable virtual tabletop for Christian-themed
PF2e play. GM (desktop, Tauri) hosts a session; players join from desktop (full CRDT
peers over Iroh P2P) or mobile (thin clients over LAN WebSocket). LLM assistant gains
session tools. One playable starter adventure ships as content.

## Hard constraints

- Existing phases keep working: `cargo test` (src-tauri) and `pnpm test` stay green.
- Heavy deps gated: `p2p` (iroh), `lan` (axum WS bridge) are cargo features, default ON,
  individually disable-able for CI/lean builds.
- Host-authoritative intents; players may only mutate their own character/token + chat.
- SQLite stays derived; canonical session = Automerge doc bytes saved to campaign folder
  (`sessions/<id>.automerge`), resumable.
- Original, respectful, ecumenical Christian content; stats validated by existing validator.
- Mind resources: no new heavy frontend deps; no extra Rust deps beyond automerge, iroh*,
  axum/tokio-tungstenite. No QR lib (show copyable join strings).

## Architecture

```
            ┌─────────────── desktop Tauri (GM host) ────────────────┐
 SvelteKit  │  session::service ─ SessionDoc (automerge)             │
 play UI ───┼── Tauri IPC (Transport=IPC)      │                     │
            │                              sync pump                │
            │  net.rs (iroh, ALPN pf2e-table/1)  ◄── QUIC ──► desktop player (full CRDT peer)
            │  lan.rs (axum WS bridge)         ◄── WS  ────► mobile companion (thin client)
            │  autosave sessions/<id>.automerge                       │
            └───────────────────────────────────────────────────────┘
```

- **CRDT of record**: `automerge` crate. One doc per session. Chat/roll log = append-only
  list. Per-player maps (character sheet, position) => per-author write domains minimize
  merges. Desktop players sync as true peers via `automerge::sync` over iroh streams.
- **Mobile thin clients**: send `Intent` JSON over WS; host validates authority, applies,
  broadcasts patches. Runs in Tauri Android/iOS shells (gen/ exists) pointed at host LAN URL.
- **Transport seam (frontend)**: `src/lib/transport.ts` — one JSON surface
  (`session_intent`, `session_subscribe`) with two backends: Tauri IPC (desktop) and
  WebSocket (mobile). Play UI only knows intents + patch events.
- **LLM**: extend `llm_tools.rs` with `roll_dice` (session-aware) + read-only tools
  (`lookup_alias`, `lookup_miracle`, `search`, `xp_budget`, `validate_statblock` exist).
  GM-only drawer in play UI reuses existing chat component; system prompt gains
  "table referee" posture (narrate via Christian lens pack, never roll for players).
- **Scenario format**: markdown adventure in existing content-pack layout
  (`data/content/lewisian/adventures/`) with YAML frontmatter `scenes:` + `handouts:` +
  NPC `.statblock.json` sidecars. Loader test asserts parse + validation.
- **Starter adventure**: "The Widow's Lamp" — original level-1 scenario (mercy/justice),
  3 scenes, 2 handouts, 3 NPC statblocks, pass via `validate_statblock`.

## Doc schema (automerge, v1)

```
meta:   { id, name, gm_id, created_at, adventure_slug? }
players:{ <peer_id>: { name, character: {level,class,hp,max_hp,ac,conditions[],notes}, color } }
chat:   [ { ts, from, kind: chat|roll|system, text, roll?: {expr, total, rolls[]} } ]
init:   { order: [<peer_id|npc_id>], active_idx, round }
scene:  { grid: {w,h,size}, background_entity_id?, tokens: { <token_id>: {owner, entity_id|pc, x, y, label} } }
handouts:{ <id>: { entity_id, visible_to: all|gm } }
```

## Intents (all validated in session::service)

CreateCharacter, UpdateSheet(patch), RollDice(expr), Chat(text), RevealHandout(id),
SetInitiative(order), AdvanceInitiative, MoveToken(id,x,y), AddToken, RemoveToken,
SetScene(grid/background), LoadAdventure(slug), EndSession.

## Verification gates (definition of done)

1. `cd src-tauri && cargo test` — all green incl.: dice parser, intent authority,
   two-peer automerge convergence, WS bridge round-trip, iroh loopback (offline-guarded),
   adventure pack parses & statblocks validate.
2. `pnpm test` + `pnpm check` green (vitest: transport mock, session store reducer).
3. `pnpm build` green; `cargo check --features p2p` green.
4. Integration test "table-demo": host + one WS client; dice roll visible on both; sheet
   edit by wrong player rejected (proves CRDT + bridge + authority).
5. README gains a "Phase 10 — Live Table" section; docs/phase10/CONTRACT.md is the
   frozen IPC/Intent/WS JSON contract both sides implement against.

## Post-review status (2026-10-14) — SHIP-WITH-NOTES

Two adversarial review rounds completed; 2 blockers + 7 should-fixes fixed and
re-verified (cargo 155/0, vitest 83/83, iroh loopback converge/resync ✔).

Known v1 limitations (next phase candidates):
1. LAN reattach is name-based — a client can claim a *disconnected* player's
   identity. Acceptable for trusted-LAN v1; add a PIN/token auth next.
2. Re-hosting a second session in the same process may serve LAN on an
   ephemeral port instead of 4780 (advertised `lan_url` is correct).
3. Chat dedupe keys on (ts, from, text) — two identical rolls in the same
   millisecond collapse. Include roll total/seq in the key next phase.
4. Secret GM rolls: totals are host-ack-only (not in the doc), but the roll
   *event* is visible. Suffices v1.
5. iroh sync verified on loopback only (`PF2E_NET_TESTS=1`); WAN/relay play
   and real Android/iOS devices untested in this session.

## Agent tasking (3 agents, distinct models)

- **A (planner/reviewer)**: critique this plan vs. the real code; write
  `docs/phase10/CONTRACT.md` (exact Intent enum, IPC + WS JSON shapes, doc schema,
  event shapes, ownership map). Final review pass uses the same session.
- **B (rust-core)**: everything under `src-tauri/` per contract + Rust tests.
- **C (frontend)**: everything under `src/` + `data/content/…/adventures/` per contract
  + vitest. Owns `src/lib/ipc.ts` and transport layer.
