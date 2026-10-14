# PLAN — Tier 1 slice: "Export session log" (SHIPPING.md item 5)

Status: PLANNED (not implemented). Source of truth: `docs/SHIPPING.md` § Tier 1
item 5. Extends the Phase 10/10.5/11 contracts; nothing in them changes.
Scope per user override: includes **post-end export via a lobby saved-sessions
list** and uses a **save-as dialog** (official `tauri-plugin-dialog`) instead
of auto-writing into the vault, with a vault-default fallback.

## 1. Recommended slice

**Save UX: "Export session log"** — a markdown transcript of a session (chat,
dice, handout reveals, roster) exported through a save-as dialog, exposed (a)
as a GM button in the live table's GM drawer and (b) from a new
saved-sessions list in the `/table` lobby that works for ended sessions and
finally wires up the currently-stubbed Resume flow.

Why: player-facing value on every table ("doubles as session notes"); the
data already lives in the automerge doc; persistence follows the existing
`save_now()` atomic tmp+rename pattern; testable headlessly with the existing
`phaseNN.rs` + vitest harnesses. Only new deps: the official Tauri dialog
plugin pair (Rust + JS, both on the project's Tauri 2 line).

Deferred candidates (one line each): **deep-links** (plugin + OS scheme
registration; needs a real device tapping a link); **log hygiene** (tracing
writer layer + session lifecycle design; natural next slice); **first-run
wizard** (implies a configurable vault root — hardcoded at
`src-tauri/src/lib.rs:73`; a feature, not a screen); **installers**
(guardrails forbid `pnpm tauri build`; per-OS bundle verification).

## 2. Implementation steps

All paths relative to repo root `pf2e-companion/`. New deps allowed: exactly
`tauri-plugin-dialog` (Rust) + `@tauri-apps/plugin-dialog` (JS), both `^2` to
match `tauri = "2"` / `@tauri-apps/api ^2`. Doc schema stays v1 (additive
serde fields with `#[serde(default)]` only, CONTRACT-10_5 §8 pattern).

### Step 1 — `src-tauri/src/session/export.rs` (new, ~160 lines)

Pure render + disk export, mirroring `service.rs` conventions:

- `pub fn render_session_log(snap: &SessionSnapshot, campaign: Option<&str>, exported_at_ms: i64) -> String`
  — pure, no I/O. Layout (all plain-string, no pulldown-cmark needed):
  - Header: `# Session: {meta.name}` + bullets — Session ID, Campaign,
    Adventure (`meta.adventure_slug` or "—"), Started (ISO-8601 UTC from
    `meta.created_at` via chrono `DateTime::from_timestamp_millis`, fixed
    format string), Exported (from `exported_at_ms`).
  - `## Table` — GM line (the `players` entry keyed by `meta.gm_id`); per
    remaining player `{name} — {character.class} {character.level}` or
    "(no character)".
  - `## Transcript` — one line per `snap.chat` entry in doc order (seq/ts
    sorted): `ChatKind::Roll` → ``**[HH:MM] from** rolls `expr` → **total**
    (dice: …)`` using `entry.roll` (`expr`/`total`/`rolls`/`kept`; mirror the
    play-page dice line at `src/routes/table/play/+page.svelte:566`;
    `roll == None` renders text only — legacy tolerance); `Chat` →
    `**[HH:MM] from:** text`; `System` → `*[HH:MM] text*`.
  - `## Handouts` — `- {id} (entity {entity_id}) — revealed` for
    `visible_to == "all"`; unrevealed (GM-only) under a separate sub-list.
- `pub fn export_log(vault: &Path, session_id: &str, dest: Option<&Path>) -> anyhow::Result<PathBuf>`
  1. `safe_segment(session_id)` — the private fn in `session/service.rs`
     (line ~477); hoist to `pub(super)` and reuse.
  2. Locate `vault/campaigns/*/sessions/{session_id}.automerge` via
     `std::fs::read_dir` (campaign from the directory name — `SessionInfo`
     carries no campaign id). Unknown id → `Err`.
  3. `SessionDoc::load(&bytes)?` → snapshot (see Risks re: visibility) →
     render with `exported_at_ms = now_ms()`.
  4. Target: `dest.unwrap_or(sessions_dir/{session_id}.log.md)`; create
     parent dirs; write atomically (`.tmp` + `std::fs::rename`, like
     `SessionService::save_now`, service.rs:224-237). Return the final path.
- `pub struct SavedSessionSummary` (serde): `{ session_id, campaign, name,
  created_at, adventure_slug: Option<String>, ended: bool, saved_at: i64 }`.
- `pub fn list_saved(vault: &Path) -> anyhow::Result<Vec<SavedSessionSummary>>`
  — scan `campaigns/*/sessions/*.automerge`; per file `SessionDoc::load` →
  snapshot meta (name, created_at, adventure_slug, ended); `campaign` from
  the directory; `saved_at` from `std::fs::metadata(..).modified()` (ms).
  **Corrupt/unloadable files are skipped with a `tracing::warn!`, never fail
  the listing.** Sort by `saved_at` descending (most recent first).
- `src-tauri/src/session/mod.rs`: `pub mod export;` + re-export
  `SavedSessionSummary` alongside the other public types.

Effort: ~2 h including tests.

### Step 2 — `ended` in the snapshot (additive, ~10 lines)

The doc writes `meta.ended` (doc.rs:67, set true on EndSession at doc.rs:398)
but `SessionMetaSnapshot` (session/mod.rs:173) doesn't read it. Add
`#[serde(default)] pub ended: bool` to `SessionMetaSnapshot`, read it in
`SessionDoc::snapshot()` (doc.rs:463; add a `get_bool` helper next to
`get_i64`/`get_str`). TS: `SessionSnapshot["meta"].ended?: boolean` in
ipc.ts. Additive + serde-default = v1 saves and wire payloads unchanged.

Effort: ~15 min.

### Step 3 — Dialog plugin (new deps)

- `src-tauri/Cargo.toml`: `tauri-plugin-dialog = "2"` (next to
  `tauri-plugin-opener = "2"`); `package.json` deps:
  `@tauri-apps/plugin-dialog: "^2"` (matches `@tauri-apps/plugin-opener ^2`).
- `src-tauri/src/lib.rs`: `.plugin(tauri_plugin_dialog::init())` next to the
  opener plugin (line ~47). No Swift/Kotlin registration needed — the plugin
  handles its own mobile init.
- `src-tauri/capabilities/default.json`: add `"dialog:default"` to
  `permissions` (it includes the `save` command permission).
- `pnpm install` + `cargo` refresh `pnpm-lock.yaml` / `Cargo.lock` — the only
  build files that may change.

Effort: ~20 min.

### Step 4 — IPC commands (`src-tauri/src/commands.rs`, ~45 lines)

Placed after `session_leave` (line ~864), same style (`Result<_, String>`,
`State<'_, …>` params):

- `session_list_saved(vault: State<'_, Arc<VaultRoot>>) -> Result<Vec<…>, String>`
  — thin wrapper over `session::export::list_saved(&vault.0)`; either re-export
  `SavedSessionSummary` or mirror it as a commands.rs DTO (project's wire-type
  pattern; tests don't care).
- `session_export_log(session_id: String, dest_path: Option<String>, vault,
  registry) -> Result<String, String>`:
  1. If `registry.get()` matches `session_id` → `active.service.save_now()`
     (flushes ≤2 s of debounced chat; EndSession already saves immediately).
  2. `dest_path.map(PathBuf::from)` → `session::export::export_log(&vault.0,
     &session_id, dest.as_deref())` → return `path.display().to_string()`.

Register both in `generate_handler!` in `src-tauri/src/lib.rs` (after
`commands::session_load`, line ~116).

Effort: ~30 min.

### Step 5 — Vault-watcher skip (KEPT, re-verified) (`src-tauri/src/vault.rs`, ~10 lines)

Re-verified during revision: `.automerge` files never trip the watcher —
`is_markdown` (vault.rs:147) matches only `.md`/`.markdown` extensions — so
they need no skip. The skip is still required because the **fallback export
path** (Step 6: mobile / dialog-unavailable) writes
`sessions/<id>.log.md` inside the vault, and the sweep (`walk` +
`is_markdown`, vault.rs:112-119) plus notify events (`handle_event`,
vault.rs:89-108) would ingest it and `ingest_file` would hard-reject it
(no `license_provenance` frontmatter) → warn churn per export.

- Add `fn is_session_artifact(path: &Path) -> bool` — any component == `sessions`.
- Guard both sweep and both `handle_event` arms:
  `is_markdown(path) && !is_session_artifact(path)`.

Effort: ~15 min + test.

### Step 6 — TS: bindings + export helper (`src/lib/ipc.ts`, `src/lib/savedSessions.svelte.ts`)

`src/lib/ipc.ts` (after `sessionLeave`, line ~456) — additive:

- `export interface SavedSessionSummary { session_id, campaign, name,
  created_at, adventure_slug: string | null, ended, saved_at }`
- `export const sessionListSaved = () => invoke<SavedSessionSummary[]>("session_list_saved");`
- `export const sessionExportLog = (sessionId: string, destPath: string | null = null) => invoke<string>("session_export_log", { sessionId, destPath });`
  (`destPath` null/omitted → vault default location; returns written path.)
- `SessionSnapshot["meta"]` += `ended?: boolean` (Step 2).

New `src/lib/savedSessions.svelte.ts` — pure helpers + shared export flow,
mirroring the `session.svelte.ts` / `lens.svelte.ts` module shape:

- `export function defaultLogFilename(name: string | null, id: string, dateMs: number): string`
  — slugified `name` (slugify-lite: lowercase, non-alphanumerics → `-`,
  collapsed; fallback: first 8 chars of `id`) + `-YYYY-MM-DD.log.md` (UTC).
- `export function describeSavedSession(s: SavedSessionSummary): string`
  — `"{name} — {campaign} • {adventure ?? 'no adventure'} • {YYYY-MM-DD}"`.
- `export async function exportSessionLogViaDialog(s: { name: string | null; session_id: string }): Promise<string | null>`
  — the shared flow used by BOTH the lobby and the GM drawer:
  1. Dynamic `import("@tauri-apps/plugin-dialog")` → `save({ defaultPath:
     defaultLogFilename(s.name, s.session_id, Date.now()), filters: [{ name:
     "Markdown", extensions: ["md"] }] })`.
  2. `dest === null` (user cancelled) → return `null`, write nothing.
  3. Dialog **throws** (unsupported platform / plugin missing) → fall back:
     `sessionExportLog(s.session_id, null)` (vault default) and return that
     path. **This is the mobile decision** (see §5): Android/iOS attempt the
     dialog (plugin-dialog 2.x supports both) but degrade to the vault
     default write on error; browser LAN clients never call this (GM-only).
  4. Otherwise `sessionExportLog(s.session_id, dest)` → return the path.

Effort: ~45 min.

### Step 7 — Lobby saved-sessions list (`src/routes/table/+page.svelte`, ~50 lines)

Replace the stubbed `handleLoad` (lines 108-121, currently
"Session load not yet implemented"):

- `let savedSessions = $state<SavedSessionSummary[]>([]);` loaded in the
  existing `onMount` via `sessionListSaved()`; `refreshSaved()` re-called
  after hosting so the list stays current.
- New "Saved sessions" card (after the join form): one row per session —
  `describeSavedSession(s)` + "ended" badge (`s.ended`) + two buttons:
  - **Resume** → `sessionLoad(s.campaign, s.session_id)` (existing, currently
    unused; `name` = `playerName`), then mirror `handleHost`'s post-join steps
    exactly (set `sessionState.info/connected`, `createTransport` + connect +
    patch/presence wiring, `goto("/table/play")` — lines 48-59).
  - **Export log** → `exportSessionLogViaDialog(s)`; show the returned path
    (or "cancelled") via the page's `error`-style status + `copyToClipboard`
    helper (line 36).

Effort: ~45 min.

### Step 8 — GM-drawer export button (`src/routes/table/play/+page.svelte`, ~15 lines)

GM drawer **Tools** tab (`gm-tools` div, line ~589), above the existing End
Session button:

- `async function exportSessionLog()` — guard `isGm && sessionState.info`;
  call `exportSessionLogViaDialog({ name: sessionState.info.name,
  session_id: sessionState.info.session_id })`; status via
  `let exportStatus = $state<string | null>(null)` + clipboard copy.
- Update `endSession`'s confirm text (line ~115): "End this session for
  everyone? Export the session log first if you want a transcript."
- Only reachable pre-end (`session_ended` clears the store,
  session.svelte.ts:27-33) — post-end export is the lobby list's job.

A11y: plain text `<button>`s — `pnpm check` stays 0/0 (CONTRACT-10_5 §5).

Effort: ~20 min.

### Step 9 — README (~8 lines)

Extend "Run your first game": step 9 mentions Resume from the lobby's saved
sessions + **Export session log** (GM drawer or lobby) → save-as dialog,
fallback path `<vault>/campaigns/<id>/sessions/<session-id>.log.md`.

Effort: ~10 min.

## 3. Test plan

**Rust — new `src-tauri/tests/phase12_session_log.rs`** (phase10_session.rs
harness: build a `SessionDoc` via `apply_intent`, tempdirs, no Tauri):

1. `render_log_includes_header_roster_chat_rolls_and_handouts` — meta() +
   GM `Chat`, player `CreateCharacter` + `Chat`, GM `RollDice { expr:
   "1d20+7", secret: false }` + `RevealHandout`; snapshot →
   `render_session_log(snap, Some("friday"), 1_700_000_060_000)`; assert
   contains session name, "friday", chat texts, `1d20+7`, the entry's own
   `roll.total` (never a literal — rolls are random), class/level line,
   revealed handout id, ISO timestamps.
2. `render_log_handles_empty_and_legacy_entries` — fresh snapshot renders
   "## Transcript" without panic; hand-built `ChatEntry { roll: None, kind:
   Roll, .. }` renders text-only.
3. `export_to_explicit_destination_writes_only_there` — doc bytes at
   `campaigns/friday/sessions/s1.automerge`; `export_log(.., Some(dest))` →
   file at `dest`, **no** `.log.md` under the vault's sessions dir.
4. `export_default_falls_back_to_vault_sessions_dir_idempotently` —
   `dest: None` → `sessions/s1.log.md`; second call overwrites, same path.
5. `export_log_rejects_unknown_and_unsafe_session_ids` — unknown id → Err;
   `"../evil"`, `"a/b"` → Err (safe_segment).
6. `list_saved_returns_meta_campaign_ended_sorted_by_mtime` — two campaigns,
   three sessions incl. one ended (`Intent::EndSession`); distinct mtimes
   via `std::fs::File::set_modified` (stable ≥1.75; rust-version 1.77) for
   deterministic order; assert name/campaign/adventure_slug/ended/saved_at
   and most-recent-first sort.
7. `list_saved_skips_corrupt_files` — a garbage `.automerge` is skipped
   (warn, not Err); valid entries still listed.
8. `ended_flag_round_trips_through_save_load` — EndSession → snapshot
   `.meta.ended == true`; save → load → still true (Step 2).

**Rust — `vault.rs` unit test** (existing `#[cfg(test)]` block):
`is_session_artifact` true for `campaigns/c1/sessions/s1.log.md`, false for
`campaigns/c1/notes.md` and for `campaigns/c1/sessions/s1.automerge` path
guard composition (`is_markdown && !is_session_artifact`).

**Vitest — new `src/lib/savedSessions.test.ts`** (colocated like
`session.svelte.test.ts`; pure helpers; mock `$lib/ipc` +
`@tauri-apps/plugin-dialog` with `vi.mock` for the flow):

- `defaultLogFilename`: slugifies ("Friday Table!" →
  `friday-table-2026-10-14.log.md`); falls back to id prefix when name
  empty/null; UTC date boundary.
- `describeSavedSession`: expected string with/without adventure.
- `exportSessionLogViaDialog` (mocked): dialog returns a path →
  `sessionExportLog(id, path)` called; returns null → no IPC call, returns
  null; dialog throws → falls back to `sessionExportLog(id, null)`.

**Gates (README "Develop")**: `cd src-tauri && cargo test` (default) and
`cargo test --all-features` 0-fail; `pnpm test`; `pnpm check` (0/0); `pnpm
build`. No env-gating — no network, no GUI automation, no device.
`PF2E_NET_TESTS`/`PF2E_NET_WAN`/`PF2E_GUI_SMOKE` untouched, still default-skip.

## 4. Acceptance criteria

1. **Live table**: GM opens **GM tab → Tools → Export session log** → a
   save-as dialog opens with default filename
   `<session-name>-YYYY-MM-DD.log.md`; the chosen file contains the session
   header (name/id/campaign/adventure/started/exported), the roster, the full
   chat transcript with dice lines (`expr → total`), and revealed/unrevealed
   handouts.
2. **Lobby**: `/table` lists saved sessions (name, campaign, adventure,
   date, ended badge). **Export log** on an ended session produces the same
   transcript without hosting. **Resume** loads it (existing `session_load`)
   and lands on `/table/play` — the "not yet implemented" stub is gone.
3. Cancelling the dialog writes nothing anywhere.
4. The last chat message sent ≤2 s before exporting is present (active
   session flush); re-export to the same destination overwrites cleanly.
5. Secret GM rolls never appear (they never enter the doc — guardrail).
6. Mobile/fallback: when the dialog errors, the log is written to
   `<vault>/campaigns/<campaign>/sessions/<session-id>.log.md` and the
   returned path is shown; that fallback file never appears in search results
   or `/vault` (watcher skips `sessions/` artifacts). `.automerge` files were
   never indexed (extension filter) — unchanged.
7. Players never see either export surface (GM drawer is GM-only; lobby list
   is host-side; LAN browser clients have no Tauri IPC).
8. Gates green: `cargo test` (+`--all-features`), `pnpm test`, `pnpm check`
   (0/0), `pnpm build`; only the two lockfiles + the two dep entries change.

## 5. Risks / unknowns

- **Mobile dialog decision (written down)**: `tauri-plugin-dialog` 2.x
  supports Android + iOS, but save pickers on mobile are the flakiest
  surface. The plan attempts the dialog everywhere and degrades on **error**
  (not cancel) to the vault-default write (Step 6.3). Browser LAN clients
  (players) never export. If mobile testing later shows the dialog is a
  hard no-op, the fallback already covers it — no code change needed.
- **Dialog plugin version pin**: Rust `tauri-plugin-dialog = "2"` beside
  `tauri-plugin-opener = "2"`, JS `@tauri-apps/plugin-dialog@^2` beside
  `@tauri-apps/plugin-opener@^2`. Capability `"dialog:default"` is required
  or every `save()` call is rejected at runtime (top failure mode).
- **Roll chat format**: read how `RollDice` appends a `ChatEntry` in
  `session/doc.rs` (`append_chat` / `take_last_roll` region) before writing
  the render code; mirror the play page's dice line. Don't guess.
- **`SessionDoc::snapshot` visibility**: check whether `snapshot()` is `pub`
  on `SessionDoc` or only reachable via `SharedDoc::with`. If private, make
  the smallest visibility change; `list_saved` + `export_log` both need it.
- **`safe_segment` reuse**: private in `session/service.rs`; make it
  `pub(super)` rather than duplicating.
- **`list_saved` cost**: loads every `.automerge` doc on lobby mount. Fine at
  friends-beta scale (tens of files); cache by mtime later if it matters.
- **Deterministic tests**: explicit `exported_at_ms` + UTC formatting; roll
  totals asserted from the entry's own `roll.total`; mtime ordering via
  `File::set_modified` (std, stable ≥1.75 — rust-version is 1.77), not sleeps.
- **Automerge doc access**: export/list never mutate the doc; the canonical
  `.automerge` is untouched (SQLite stays derived; the exported `.md` is a
  derived artifact wherever the user puts it).
- **New deps are exactly two**: the dialog plugin pair. No `filetime` (std
  `set_modified` suffices), no `rand`, nothing else.
