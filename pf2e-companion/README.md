# pf2e-companion

Cross-platform PF2e Remaster + Christian Biblical worldview reference & worldbuilding companion. Tauri 2 + SvelteKit + SQLite (FTS5 + sqlite-vec).

> **Status**: Phase 11 — Live Table (real-time multiplayer) + second adventure + a11y cleanup.
> Shipping roadmap to a broadly-playable release: `docs/SHIPPING.md`.
> Plan: `~/wiki/topics/pf2e-worldbuilding-tool/output/plan-cross-platform-pf2e-biblical-reference-2026-05-25.md`.

## What works

### Phase 0 — scaffold (done)
- Tauri 2 + SvelteKit + TypeScript skeleton.
- SQLite schema, FTS5, sqlite-vec extension registered via `sqlite3_auto_extension`.
- Vault watcher (notify-rs) starts on launch.
- IPC: `search`, `list_entities`, `schema_version`.

### Phase 1 — reference layer (done)
- **Seed extraction**: `scripts/extract_seeds.py` parses the wiki reference articles into JSON. **161 Remaster aliases + 71 miracle-spell rows** produced; embedded via `include_str!` so the binary ships the data.
- **Reference data lookups**: `lookup_alias("Magic Missile")` → `Force Barrage`; `lookup_miracle("Mt 14:25")` → *Water Walk* (Gospels). Aasimar/Tiefling/Aphorite/Ganzi all resolve to **Nephilim**. Mithral → **Dawnsilver**.
- **Encounter math**: `xp_budget` matches PF2e Remaster GM Core p.49 (party of 4: 40/60/80/120/160 XP for trivial/low/moderate/severe/extreme). `creature_xp` returns the level-vs-PL XP cost table.
- **Statblock validator**: structural sanity check + sanctification taxonomy enforcement (5 Remaster values incl. `none`, the Pharasma escape hatch).
- **Lens manifest**: `list_lenses()` returns the 5 v1 lens packs (Lewisian default + Catholic / Reformed / Pentecostal / Orthodox).
- **Markdown vault ingestion**: the watcher now parses YAML frontmatter, slugifies IDs, attaches sibling `.statblock.json`, indexes into FTS5, and propagates updates/deletes.
- **Foundry-pf2e ingest stub**: `import_foundry_pack(path, license)` walks a packs directory and inserts `entities` rows tagged with the chosen license posture.
- **UI**: 4 pages — `/` search with type filter; `/encounter` XP budget calculator + creature-XP table; `/aliases` Remaster name lookup; `/miracles` Bible-reference → spell. Lens picker in the top bar; bottom-tabs nav for mobile.

### Phase 2 — Lewisian content pack (done)
- **Bundled content** — `data/content/lewisian/` packaged into the binary via `include_dir!`. **16 entries**: 1 deity (YHWH with full Remaster stat block JSON sidecar), 4 archangels (Michael, Gabriel, Raphael, Uriel — Uriel flagged as the deuterocanonical "swing case"), 6 cosmology entries (Heaven, Sheol, Gehenna, Tartarus, Abyss, New Jerusalem), 5 class reskin notes (Champion, Cleric, Oracle, Thaumaturge, Sorcerer).
- **Backend**: `content::load_bundled_packs` runs at startup; entries indexed under `source = 'reference'`, lens = `'lewisian'`, and surfaced through the existing search.
- **`get_entity(id)` IPC** for the detail page.
- **Detail page**: `/entity/[id]` route renders markdown body + sidebar with stat block card + sources frontmatter. `[[id]]` wikilinks in markdown are rewritten to internal hrefs. Search hits and tab targets all link through.
- **`<Statblock />` component** for deity stat block cards (edicts/anathema/sanctification/domains/cleric spells/iconography).

### Phase 5 — all 5 lens packs (done)
- **Lewisian** (16 entries; v1 default; mere-Christianity)
- **Catholic** (35 files): full 9-choir hierarchy, **20 saints** (Mary, Joseph, the Twelve Apostles, four archangels, Francis-of-Assisi, Thérèse-of-Lisieux), **purgatory** as a transitional plane, sacramental cleric + relic-pilgrim Thaumaturge with the formal first/second/third-class relic taxonomy.
- **Reformed** (20 files): 66-book canon, cessationism front-and-center in cleric.md and pack metadata, 6 covenant-figure exemplars (NOT saints — explicit "exemplary witness, not patron" block in every entry), no purgatory, Word-as-sword Thaumaturge, dedicated `reference/covenants.md` (Adamic/Noahic/Abrahamic/Mosaic/Davidic/New).
- **Pentecostal** (21 files): strong continuationism, 6 Bible-hero exemplars (Elijah, Daniel, Paul, Philip the Evangelist + Michael, Gabriel), three-tier demonology (Tartarus / Abyss / Gehenna), dedicated `reference/charismata.md` mapping the 9 gifts of 1 Cor 12:8-10 to PF2e Divine spells.
- **Orthodox** (24 files): Septuagint+ canon, **Synaxis of Seven archangels** (Michael, Gabriel, Raphael, Uriel + Selaphiel, Jegudiel, Barachiel), `cosmology/aerial-toll-houses.md` as a campaign-grade level-15-to-20 mega-dungeon with 20 toll-stations, dedicated iconographer/hesychast `classes/monk.md` with the Jesus Prayer as a continuous Focus-spell mechanic, `reference/theosis.md` with a Tabor-light level-20 capstone.

**Total: 116 content entries across 5 packs**; FTS surfaces lens-distinctive terms cleanly (`purgatory` → Catholic only; `toll` → Orthodox; `covenant` → Reformed; `charism` → Pentecostal). Three new tests in `tests/phase5.rs` (`all_five_lens_packs_load`, `lens_specific_distinctives_present`, `fts_surfaces_distinctive_content_per_lens`).

### Phase 4 — mobile co-targets (initialized)
- **iOS**: `pnpm tauri ios init` produced a clean Xcode project at `src-tauri/gen/apple/`. Bundle ID `io.github.gkrause.pf2e-companion`. Both iOS Rust targets (`aarch64-apple-ios`, `aarch64-apple-ios-sim`) cross-compile cleanly with the SQLite + sqlite-vec stack. `xcodebuild` itself isn't run in this phase — that needs a signing identity, which is Phase 8's territory.
- **Android**: `pnpm tauri android init` produced a Gradle project at `src-tauri/gen/android/`. NDK 27.0, JDK 21 (Temurin), `compileSdk` 34. Android-side build orchestration is owned by Tauri's Gradle plugin (it manages the per-API-level NDK toolchain that bare `cargo check --target aarch64-linux-android` doesn't know about).
- **Mobile env note**: `~/.zshrc` exports `ANDROID_HOME`, `NDK_HOME`, `JAVA_HOME` and adds Android tools to `PATH`. SDK licenses accepted.
- **CI matrix** at `.github/workflows/build.yml`: cargo test on macOS first; then a 4-target desktop matrix (macOS arm64 + x86_64, Linux x86_64, Windows x86_64), iOS cross-compile sanity, and Android debug APK build. All build-only — signing is Phase 8.
- **What still needs a real device session**: iOS Simulator + Android Emulator hands-on UX shake-down, TestFlight + Play internal track upload, signing certificates.

### Phase 6.5 — Eval harness (done)
- **Bundled prompt suite** at `src-tauri/data/eval/prompts.json` — 6 starter cases covering tool-call coverage (xp_budget, lookup_alias, lookup_miracle, search-with-lens, validate_statblock) and the DragonRaid-trap discipline check.
- **Tiny assertion DSL** in `src-tauri/src/eval.rs`: `tool_called`, `tool_called_with` (recursive JSON-superset match, case-insensitive on strings), `text_contains`, `text_excludes`, `iterations_at_most`. Adding a new check is a `match` arm + a `Variant` — kept narrow on purpose.
- **Pure grading function** (`grade_one`) — a `RunRecord` plus a `Prompt` produces a `RunResult`. No I/O, deterministic. Heavy-lifting tests in `phase6_5.rs` exercise the grader against scripted agent runs without touching a live LLM.
- **`/settings/eval` UI**: Run-suite button, color-coded per-prompt cards with PASS/FAIL badges, expandable run details (tool-call list + final text), top-of-page summary. Linked from `/settings/llm`.
- **6-test phase6_5.rs**: bundled-suite shape, full pass with a "good agent", failure detection with a "bad agent", DragonRaid-trap honest/dishonest discrimination, JSON-contains nested matching, expectation serde round-trip.

### Phase 6 § D — Tool-use loop + agent surface (done)
- **Agent loop** in `src-tauri/src/llm_tools.rs`: streams tokens to the UI, intercepts `ToolCall` chunks, executes locally, appends `Tool` messages, and recurses. Hard cap at 5 iterations.
- **Five tools wired** off the existing Phase 1/6-§-C surfaces (no duplication): `xp_budget`, `validate_statblock`, `lookup_alias`, `lookup_miracle`, `search` (the hybrid one).
- **LLM trait extended** with `ToolCall` / `ToolSpec` / `ToolCallResult` plumbing. Provider-side: Anthropic streams `tool_use` blocks via `content_block_start` + `input_json_delta`; Ollama uses its native `tool_calls` field. Both translate the trait's flat `Message` shape into the right native content-block layout for tool results (`tool_result` for Anthropic, `tool_calls` for Ollama).
- **Anthropic prompt caching** opt-in via `cache_system: true` on `ChatOpts` — the system block is sent with `cache_control: ephemeral`, and `cache_read_input_tokens` / `cache_creation_input_tokens` flow back through the usage summary so the chat UI can show a "cache hit" badge.
- **Chat UI** gains a per-assistant-turn `<details>` trace (tool name + args summary + collapsible JSON result) and a "Show context (tool calls)" toggle. Usage line shows tokens, iterations, and cache-hit count.
- **`llm_chat` IPC reshaped** as a discriminated union (`token` / `tool_start` / `tool_result` / `done` / `error`) so the frontend can render the agent's side-effects without polling.
- **9-test phase6_d.rs** covers dispatch routing for every tool, end-to-end multi-turn agent flow with a scripted in-process provider, the 5-iteration cap, and tool-error recovery.

### Phase 6 § C — RAG over the bundled content (done)
- **`entities_vec` lit up.** `src-tauri/src/rag.rs` chunks each reference entity by paragraph (≈150-500 token target via a char-count proxy), embeds via the active provider, and writes float[768] vectors keyed by rowid. `embeddings_meta` (new in schema v3) carries the (entity_id, chunk_idx, chunk_text, provider, model) tuple.
- **Hybrid search** rewrites `commands::search` to delegate to `rag::hybrid_search`, which fuses FTS5 and vector hits via reciprocal-rank fusion (RRF, k=60). When the LLM provider isn't configured or the corpus isn't indexed, it transparently falls back to FTS-only — the IPC contract is unchanged.
- **`/settings/llm` reindex card.** New "Reindex content" button calls the `rag_reindex` IPC; status row shows `entities × chunks` indexed plus the provider/model lineage.
- **Search UI** now badges hits as `vec` (semantic-only) or `fts+vec` (both retrievers).
- **Embeddings posture**: Anthropic doesn't ship an embedding API, so reindexing requires Ollama. The error message tells the user to switch providers + pull `nomic-embed-text` (or any 768-dim embedder) before retrying.
- **5-test phase6_c.rs** with a deterministic in-process `FakeEmbedder` provider exercises the full embed → vector-search → RRF fusion path on a 3-entry mini corpus, plus FTS-fallback semantics and re-embed idempotency.

### Phase 6 § B — LLM provider scaffolding (done)
- **Off by default, BYO key.** No provider runs at startup. Configure at `/settings/llm`.
- **Two providers behind a single trait** (`src-tauri/src/llm.rs`): `AnthropicProvider` (SSE on `/v1/messages`) and `OllamaProvider` (NDJSON on `/api/chat` + `/api/embeddings`). Anthropic does not provide embeddings — its `embed()` errors; RAG indexing in Stage C uses Ollama for the embed half.
- **Keys in the OS keychain** (`src-tauri/src/keystore.rs` over the `keyring` crate: macOS Keychain / Windows Credential Manager / libsecret). Never persisted in plaintext.
- **Streaming chat at `/chat`**. The IPC `llm_chat` returns a session id and emits `llm:token` Tauri events; the Svelte page subscribes and appends tokens to the assistant slot. System prompt encodes the active lens and re-asserts the DragonRaid-trap rule.
- **Disclaimer surface**: yellow banner on `/chat`, full disclaimer on `/settings/llm`, paragraph in `/about` under "design discipline".
- **`SseBuffer`** is a pure parser with 5 unit tests + a 7-test integration suite (`tests/phase6_b.rs`) that covers registry lifecycle, provider guard rails, and a realistic Anthropic stream chopped into 17-byte fragments.

### Phase 3 — vault editor (done)
- **Vault CRUD backend** (`src-tauri/src/vault_write.rs`): `list_campaigns`, `create_campaign`, `create_entity`, `update_entity` (with rename support that propagates filename changes), `delete_entity`, `add_relation`, `delete_relation`, `list_relations`. Entity writes go through the markdown vault on disk; the notify-rs watcher re-indexes via the existing Phase 1 ingestion path.
- **Path safety**: every campaign/type/title segment is validated against traversal (`..`), absolute paths, and dotfile prefixes before any filesystem write.
- **License-provenance enforcement** at write time — strict allowlist of `orc | community-use | homebrew | proprietary`.
- **`/vault` route**: campaign picker as cards, per-campaign entity list filtered by type and lens, "+ New entity" button, inline create-campaign form with a lens selector.
- **Top-bar campaign picker** alongside the lens picker (`src/lib/campaign.svelte.ts` shared state).
- **`/vault/new` and `/vault/[id]`** routes share an `<EntityEditor />` component: title / type / campaign / lens / license fields; **CodeMirror 6 markdown editor** for the body; extra-frontmatter JSON textarea; relations editor with add/remove (edit mode only); save / delete buttons.
- **Foundry-pf2e end-to-end test** (Phase 1 carry-over): `tests/phase3.rs::foundry_e2e_round_trips_a_synthetic_pack` builds a synthetic packs/spells directory in a tempdir, ingests it, and asserts that records land with `source = 'reference'` and `license_provenance = 'orc'`.

### Tests
**42 tests passing** (`cargo test`):
```
running 12 tests             [unit tests in lib]
test rules::tests::xp_budget_canonical_party_of_4 ... ok
test rules::tests::xp_budget_scales_with_party_size ... ok
test rules::tests::creature_xp_table ... ok
test rules::tests::validate_minimal_statblock ... ok
test rules::tests::validate_rejects_bad_sanctification ... ok
test rules::tests::validate_warns_on_missing_license ... ok
test vault::tests::split_frontmatter_basic ... ok
test vault::tests::split_frontmatter_missing_returns_body_only ... ok
test vault::tests::slugify_collapses_punctuation ... ok
test vault::tests::strip_markdown_keeps_text_drops_syntax ... ok
test foundry::tests::foundry_record_extraction ... ok
test foundry::tests::license_posture_strings ... ok

running 2 tests              [tests/phase1.rs]
test seeds_load_and_canonical_lookups_resolve ... ok
test xp_budget_matches_pf2e_gm_core ... ok

running 1 test               [tests/smoke.rs]
test schema_migrates_seeds_and_searches ... ok
```

## What's not yet in the app (intentional, by phase)

- **Phase 7** — Foundry export round-trip + plugin SDK.
- **Phase 8** — App-store submission + signing pipeline.

## Content provenance

Every entity ships a `sources:` array citing the wiki article it was authored
from plus a Bible reference or doctrinal primary source. The in-app `/about`
page explains the citation conventions, the design discipline (the DragonRaid
trap), and the four license-provenance badges. The research wiki itself lives
at `~/wiki/topics/pf2e-biblical-reskin/` and `~/wiki/topics/pf2e-worldbuilding-tool/`
on the maintainer's machine — PRs that update wiki sources should flow into
content-pack updates the same way the Phase 5 lens packs were authored.

## Develop

Prerequisites: Rust 1.77+, Node 20+, pnpm. Python 3 for the seed extractor.

```bash
cd pf2e-companion
pnpm install
pnpm tauri dev          # launches the app window with HMR

# Re-extract seeds when the wiki reference articles change:
python3 scripts/extract_seeds.py

# Run the frontend unit tests (Vitest, ~200ms):
pnpm test

# Run the Rust test suite (~2s):
cd src-tauri && cargo test

# Production build (no installer):
pnpm tauri build --no-bundle
```

### Env-gated tests (off by default)

| Env var | Test | Purpose |
|---|---|---|
| `PF2E_GUI_SMOKE=1` | `gui_smoke_launches_cleanly_under_xvfb` | Launches built app under `xvfb-run`, verifies HTTP 200 on `localhost:1420`, no panics in 45s. Requires `xvfb-run`, `curl`, `pnpm`. |
| `PF2E_NET_WAN=1` | `relay_peers_converge_via_n0_relay` | iroh P2P via N0 relay (QUIC/UDP 443), 10s timeout. Fails with guidance if sandbox blocks relay. |
| `PF2E_NET_TESTS=1` | `loopback_peers_converge_and_resync_on_change` | Local iroh P2P loopback convergence (existing). |

Default `cargo test` runs NEITHER gui nor WAN test — both early-return without their env vars.

## Architecture

```
SvelteKit (src/) ─── IPC ─── Rust core (src-tauri/src/)
  routes/                        ├── db.rs        SQLite + sqlite-vec, schema v2
    +layout.svelte                │                + Phase 1 seeds (aliases, miracles)
    +page.svelte (search)         ├── commands.rs IPC: search/lookup/budget/validate/import
    encounter/+page.svelte        ├── rules.rs    XP budget, creature XP, statblock validation
    aliases/+page.svelte          ├── vault.rs    notify-rs + markdown frontmatter ingest
    miracles/+page.svelte         ├── foundry.rs  Foundry-pf2e JSON import (stub)
  lib/                            └── lib.rs      Tauri setup
    ipc.ts (typed bindings)
    lens.svelte.ts (state)
data/seeds/
  remaster_aliases.json     [161 rows]
  miracle_spell_map.json    [71 rows]
scripts/
  extract_seeds.py          [wiki .md → JSON]
```

Vault data is plain markdown + JSON in a user-chosen folder. SQLite is a derived index, never canonical. License-provenance (`orc | community-use | homebrew | proprietary`) is tracked per record and validated on ingest.

Full spec: `~/wiki/topics/pf2e-worldbuilding-tool/output/plan-cross-platform-pf2e-biblical-reference-2026-05-25.md`.

## Run your first game

1. **Host**: Open `/table` in the Tauri app. Enter your name, pick a color, choose a campaign (create one in `/vault` if needed), optionally pick "The Widow's Lamp" adventure, click **Host Session**.
2. **Share**: The lobby shows a **P2P Ticket** (desktop peers), a **LAN URL** (mobile), and a **Table PIN** (6 chars, required for LAN joins). Copy and send the appropriate invite.
3. **Join**: Desktop peers paste the ticket on `/table` → **Join Session**. Mobile users open the LAN URL (e.g., `ws://192.168.1.42:4780/abc123?pin=ABC123`) in browser or Tauri Android/iOS app.
4. **Enter Table**: Host clicks **Enter Table** → `/table/play`. Players arrive at the same play surface (6 tabs).
5. **Load Adventure**: GM opens **GM** tab → **Load Widow's Lamp** (or **The Roof of Mercy**). Adventure loads: scenes, handouts, NPCs, tokens.
6. **First Roll**: In **Chat+Dice** tab, type `/roll 1d20+7` → Enter. Result appears with per-die breakdown.
7. **Secret GM Roll**: In **GM Drawer** (GM tab or overlay), type `/roll 2d6+3 --secret` → Enter. Roll is hidden from players, visible only to GM.
8. **Handout Reveal**: In **Handouts** tab, GM clicks **Reveal to Players** on a handout. All players see it immediately.
9. **End / Resume**: GM clicks **End Session** → confirms. Later, host uses the **Saved Sessions** list on `/table` to **Resume** from the saved automerge document.
10. **Export session log**: From the GM drawer (**GM → Tools → Export session log**) or the lobby's **Saved Sessions** list, export a markdown transcript (header, roster, chat + dice, handouts) via a save-as dialog. If the dialog is unavailable (e.g. some mobile setups), the log falls back to `<vault>/campaigns/<id>/sessions/<session-id>.log.md` and the path is shown. Cancelling writes nothing.

## Phase 6 next steps

1. **LLM client** in the Rust core (`src-tauri/src/llm.rs`): Anthropic + Ollama providers behind a single trait; BYO-key onboarding flow.
2. **RAG over the bundled content** via the existing sqlite-vec virtual table — chunk + embed entities at startup; hybrid FTS + vector at query time.
3. **Tool-use loop** with the PF2e validators we already have (`xp_budget`, `validate_statblock`, `lookup_alias`, `lookup_miracle`).
4. **Anthropic prompt caching** for the always-on world-bible (per the wiki finding: ~10× cost reduction on 60K-token canon).
5. **Eval harness** for canon-faithfulness and statblock validity.

## Mobile development (Phase 4 → ongoing)

For iOS/Android dev you'll need:

```bash
# In ~/.zshrc (already added if you ran Phase 4):
export ANDROID_HOME="$HOME/Library/Android/sdk"
export NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
export JAVA_HOME="/Library/Java/JavaVirtualMachines/temurin-21.jdk/Contents/Home"
export PATH="$JAVA_HOME/bin:$ANDROID_HOME/platform-tools:$PATH"

# iOS Simulator (no signing identity required):
pnpm tauri ios dev

# Android emulator / device:
pnpm tauri android dev
```

## Phase 10 — Live Table

Real-time shared tabletop for PF2e. Desktop hosts via Tauri IPC; mobile clients join via LAN WebSocket (port 4780) or P2P ticket (iroh). One codebase, works unchanged in Tauri Android/iOS shells.

### What it does
- **Lobby (`/table`)**: Host a new session (pick campaign, adventure, your name/color) or join via P2P ticket or host LAN URL. Shows copyable invite ticket and LAN URL.
- **Play surface (`/table/play`)**: Six tabs — Chat+Dice (dice parser `/roll 1d20+7 --secret` with per-die breakdown), Initiative tracker (GM reorder/advance), Party (character sheet cards, owner-editable), Scene (square grid canvas, drag tokens you own, GM adds/removes/sets background), Handouts (markdown render, GM reveals), GM drawer (reuses chat component, auto-gets `roll_dice` tool).
- **Mobile layout**: Reuses existing bottom-tabs nav; compact tabs with icons-only on narrow screens.
- **Authority**: GM-only intents (reveal handout, set initiative, add/remove token, set scene, load adventure, end session) enforced by backend; players only move their own tokens, edit their own sheets.

### Host / Join (Desktop)
1. Open `/table` in the Tauri app.
2. Enter your name, pick a color.
3. Choose a campaign (create one in `/vault` if needed).
4. Optionally pick "The Widow's Lamp" adventure.
5. Click **Host Session**.
6. Copy the **P2P Ticket** (for desktop peers) or **LAN URL** (for mobile) and share.
7. Click **Enter Table** to join your own session.

### Join (Mobile / Second Desktop)
- **P2P (desktop)**: Paste the ticket into the "P2P Ticket" field on `/table`, enter your name, click **Join Session**.
- **LAN (mobile)**: Open the LAN URL (e.g., `ws://192.168.1.42:4780/abc123`) in the mobile browser or Tauri Android/iOS app. The `?host=` query param is auto-handled. **LAN sessions are PIN-protected** — the host lobby displays a 6-character Table PIN next to the LAN URL; joiners must enter it.

### Feature flags (Rust, `src-tauri/Cargo.toml`)
- `lan` (default ON) — WebSocket bridge on port 4780 (`tokio-tungstenite`).
- `p2p` (default ON) — iroh 1.3 P2P sync (`iroh`, `iroh-tickets`).
- CI lean path: `cargo check --no-default-features` compiles neither.
- Offline guard: P2P tests require `PF2E_NET_TESTS=1`.

### Adventure content
- `src-tauri/data/content/lewisian/adventures/the-widows-lamp/widows-lamp.md` — Level 1–2 scenario, 3 scenes, 2 handouts, 4 NPC statblocks.
- `src-tauri/data/content/lewisian/adventures/the-roof-of-mercy/roof-of-mercy.md` — Level 3–4 scenario, 3 scenes, 1 handout, 3 NPC statblocks (Mark 2:1–12 forgiveness theme).
- Statblock sidecars pass `validate_statblock` (sanctification taxonomy, `license_provenance: homebrew`).
- `sources:` arrays cite Bible refs and the Lewisian wiki; tone aligns with the pack's mere-Christianity posture.

### Tests
- `pnpm test` — Vitest: `transport.test.ts` (mock transport, WS frame shapes), `session.svelte.test.ts` (reducer: snapshot replace, chat ordering, token merge, handout reveal, session end, unknown-kind tolerance).
- `cargo test` — Rust: dice parse/roll, session authority matrix, automerge sync convergence, adventure parsing/validation, LAN WS hello→welcome→snapshot + chat broadcast + authority rejection, P2P loopback convergence (guarded by `PF2E_NET_TESTS=1`).

### Architecture additions
```
src/
  lib/
    ipc.ts          + Phase 10 types (Intent, PatchEvent, SessionSnapshot, SessionInfo, sessionHost/Join/Load/Intent/Leave)
    transport.ts    + two-backend Transport interface (ipcTransport, wsTransport, mockTransport, createTransport)
    session.svelte.ts  reactive store (applyPatch reducer, derived state, sendIntent)
  routes/
    table/+page.svelte           lobby (host/join/invite, saved sessions)
    table/play/+page.svelte      play surface (6 tabs, grid, dice, sheets)
```

## Support

Development tips appreciated: **average@minibits.cash** (Lightning Address, via the repo's Sponsor button).

Tips only — no store, no accounts, nothing in-app. `docs/research/nostr-integration-analysis.md` explains why monetization deliberately stays outside the app (app-store policy + the out-of-scope list in `docs/SHIPPING.md`).
