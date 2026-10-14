# PLAN — Tier 1 slice: "Crash + log hygiene" (SHIPPING.md item 4)

Status: PLANNED (not implemented). Source of truth: `docs/SHIPPING.md` § Tier 1
item 4: "tracing already present — add a ring-buffer log file per session
under the vault dir + 'Copy diagnostics' button in Settings (paste-ready for
GitHub issues). No telemetry yet." Extends Phase 12 (committed `02acd14`);
nothing in prior contracts changes.

## 1. Recommended slice (re-evaluated)

**Confirm log hygiene** as the next slice, unchanged from the earlier deferral
note in `docs/plans/PLAN-TIER1-session-log-export.md`.

Why: it is the only remaining Tier 1 candidate that combines (a) direct
friends-beta value — beta testers are non-technical friends; "Copy
diagnostics" turns "it broke during the boss fight" into an actionable GitHub
issue, which is the feedback loop the whole tier depends on; (b) **zero new
dependencies** — `tracing` + `tracing-subscriber` are already initialized in
`src-tauri/src/lib.rs:38-44` and `chrono` is present for timestamps; (c) full
automatability — pure Rust writer/hub logic, thread-local-dispatch integration
tests, one vitest helper; no device, no network, no GUI automation.

Re-evaluation against the alternatives:

- **Deep-links (item 2)** — highest remaining user value, but: new
  `tauri-plugin-deep-link` dep (and the Phase 12 review just demonstrated the
  platform-scoping pitfalls of Tauri plugins — the dialog plugin needed
  Cargo feature fixes); OS scheme registration; and verification fundamentally
  needs a real device tapping a link (Tier 1 item 1's evening). Defer and pair
  it with the real-device session where it belongs.
- **First-run wizard (item 6)** — the vault-root screen implies configurable
  vault storage; today it is hardcoded (`app_data/vault`, lib.rs:74) and
  moving it restarts the notify-rs watcher and the DB ingest path. A real
  feature plus UX decisions only the maintainer can make. Still too big.
- **Release installers config (item 3)** — config-only is plannable but has
  no verifiable payoff inside the automation guardrails (`pnpm tauri build`
  is forbidden); value materializes only when the friends-beta build is
  actually cut, so it belongs to that moment, not to a code slice.

Nostr note: `docs/research/nostr-integration-analysis.md` (verdict: reject
from core) is irrelevant to this slice; it only touches Tier 3
federation/rendezvous discussions (SHIPPING items 17–18). Cited here for
completeness only.

## 2. Implementation steps

All paths relative to repo root `pf2e-companion/`. **No new cargo or npm
deps.** Doc schema, wire formats, and the automerge doc are untouched.

### Step 1 — `src-tauri/src/diagnostics.rs` (new, ~170 lines)

The bounded in-memory ring is the "ring buffer"; the per-session file under
the vault is its durable twin. Both live in one hub:

```rust
pub struct DiagnosticsHub {
    ring: Mutex<VecDeque<String>>,          // cap = 2_000 lines (~400 KB max)
    session: Mutex<Option<SessionSink>>,    // current session's file, if any
}
struct SessionSink { path: PathBuf }
```

- `impl DiagnosticsHub { pub fn new(cap: usize) -> Arc<Self>; pub fn push_line(&self, line: &str) }`
  — appends to the ring (evicting oldest past cap) AND, when a session sink
  is set, appends the line to the session file (plain `fs::write` append via
  `OpenOptions::append`; file IO errors are swallowed — diagnostics must
  never take the app down).
- `pub fn set_session(&self, path: &Path)` — record the sink; append a
  `=== session log started <ISO-8601 UTC> ===` marker line (append mode, so
  a resumed session keeps earlier history below a fresh marker).
- `pub fn clear_session(&self)` — drop the sink (ring keeps rolling).
- `pub fn snapshot_lines(&self) -> Vec<String>` — ring copy.
- `pub fn blob(&self, active_session_id: Option<&str>) -> DiagnosticsInfo`
  where `DiagnosticsInfo` (serde) = `{ app_version: env!("CARGO_PKG_VERSION"),
  os: std::env::consts::OS, arch: std::env::consts::ARCH,
  session_id: Option<String>, lines: Vec<String> }`.
- Global instance: `static HUB: OnceLock<Arc<DiagnosticsHub>>` + `pub fn hub()
  -> &'static Arc<DiagnosticsHub>` (the app's hub; tests build their own via
  `new` so they share no state).
- Tracing adapter (what makes events flow in): a `TeeWriter` implementing
  `for<'a> tracing_subscriber::fmt::MakeWriter<'a>` whose `make_writer()`
  returns a handle that (1) mirrors raw bytes to `std::io::stderr()` (keeps
  today's console behavior byte-identical) and (2) buffers bytes until `\n`,
  then calls `hub.push_line(complete_line)`. Partial writes without a newline
  stay buffered (fmt layers may emit a line in several `write` calls).
- `src-tauri/src/lib.rs`: `pub mod diagnostics;` in the module list.

Effort: ~2 h including tests.

### Step 2 — Wire the subscriber (`src-tauri/src/lib.rs`, ~10 lines changed)

Replace the init at lines 38-44 with the same `fmt()` layer, same
`EnvFilter` default string, but `.with_writer(...)` pointing at a `TeeWriter`
bound to `diagnostics::hub()`. One fmt layer with a tee writer — not a
layered registry — keeps the diff minimal and console output unchanged.
Keep `.try_init().ok()` semantics (never panic if a subscriber exists).

Effort: ~15 min.

### Step 3 — Session lifecycle hooks (`src-tauri/src/session/service.rs`, ~10 lines)

- `host()` (line ~491): after `let save_path = dir.join(...)` (line 514) →
  `crate::diagnostics::hub().set_session(&save_path.with_extension("log"));`
  → `<vault>/campaigns/<campaign>/sessions/<id>.log`.
- `load()` (line ~595): same call next to its save path construction.
- `SessionService::shutdown()` (line 124) → `crate::diagnostics::hub().clear_session();`
  — `shutdown()` is reached by every teardown path (`take`, `take_and_shutdown`,
  `install`'s old-session branch), so one call covers all of them. Ordering is
  safe: teardown clears before the next `host()`/`load()` sets.
- `join_peer()` (save_path `None`) intentionally logs to the ring only — no
  per-session file for peers; document in the module doc comment.
- **Watcher safety, re-verified**: the file extension is `.log`, and
  `is_markdown` (vault.rs:147) matches only `md`/`markdown` — the vault
  watcher never ingests it. Phase 12's `is_session_artifact` skip
  (vault.rs:157) additionally covers any `.log.md` in `sessions/`. **No
  `vault.rs` changes in this slice.**

Effort: ~20 min.

### Step 4 — IPC command (`src-tauri/src/commands.rs`, ~15 lines)

After `session_export_log` (line ~879), same style:

```rust
#[tauri::command]
pub fn diagnostics_get(
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<session::…::DiagnosticsInfo, String> {
    let active = registry.get().map(|s| s.handle.session_id.clone());
    Ok(crate::diagnostics::hub().blob(active.as_deref()))
}
```

(`DiagnosticsInfo` lives in `diagnostics.rs` and is re-exported; commands.rs
does not define a duplicate DTO.) Register `commands::diagnostics_get` in
`generate_handler!` in lib.rs (after `commands::session_export_log`, line
~119).

Effort: ~15 min.

### Step 5 — TS bindings + paste formatter (`src/lib/ipc.ts`, new `src/lib/diagnostics.ts`)

`ipc.ts` (after `sessionExportLog`):

```ts
export interface DiagnosticsInfo {
  app_version: string; os: string; arch: string;
  session_id: string | null; lines: string[];
}
export const diagnosticsGet = () => invoke<DiagnosticsInfo>("diagnostics_get");
```

New `src/lib/diagnostics.ts` — pure, mirrors `citation.ts` (plain module +
colocated test):

- `export function formatDiagnosticsMarkdown(info: DiagnosticsInfo): string`
  — paste-ready GitHub blob:
  ```
  ### pf2e-companion diagnostics
  - Version: … / OS: … / Arch: … / Active session: …
  <details><summary>Last N log lines</summary>

  ```
  (one line per entry)
  ```
  </details>
  ```

Effort: ~30 min.

### Step 6 — Settings page (`src/routes/settings/+page.svelte` new, `src/routes/+layout.svelte` ~2 lines)

There is no `/settings` index today (only `/settings/llm`, `/settings/eval`,
reachable via links from `/chat` and `/about`). Create one:

- **Diagnostics card**: one-paragraph explainer ("no telemetry — this stays
  on your machine until you paste it"), a **Copy diagnostics** button →
  `diagnosticsGet()` → `formatDiagnosticsMarkdown()` → clipboard
  (`navigator.clipboard.writeText`, same pattern as the table lobby's
  `copyToClipboard`); status line "Copied — paste into a GitHub issue". On
  clipboard failure, reveal a `<details><textarea readonly>` fallback with
  the same text so it can be selected manually (and useful on mobile).
  Show a live count ("last N events captured") from `info.lines.length`.
- Links card to `/settings/llm` and `/settings/eval` (discoverability —
  today they're only linked from /chat and /about).
- `+layout.svelte`: add a `Settings` `about-link` pill next to About/Chat
  (line ~27-28) pointing at `/settings`.

A11y: labeled buttons + textarea; `pnpm check` stays 0 errors / 0 warnings.

Effort: ~45 min.

### Step 7 — README (~8 lines)

Document under "Develop" or a small "Diagnostics" note: Copy diagnostics at
`/settings`, what it contains (version/OS/session + last 2,000 log lines),
per-session `<vault>/campaigns/<id>/sessions/<id>.log`, and the no-telemetry
posture.

Effort: ~10 min.

## 3. Test plan

**Rust — unit tests in `diagnostics.rs`** (`#[cfg(test)]`, standalone hubs —
no global state):

1. `ring_evicts_oldest_past_cap` — cap 3, push 5 → last 3 lines only.
2. `fragmented_writes_assemble_into_lines` — write `"par"`, `"tial\nsec"`,
   `"ond\n"` through a `TeeWriter` handle → ring holds `"partial"`,
   `"second"` (no partial-line entries).
3. `session_sink_appends_and_marker_written` — tempdir sink; `set_session`
   writes the marker; `push_line` lands in the file; `clear_session` stops
   file appends (ring keeps rolling).
4. `blob_contains_app_metadata` — `blob(Some("s1"))` carries
   `app_version`/`os`/`arch`/`session_id`/lines.
5. `session_sink_survives_unwritable_file` — sink pointing at an impossible
   path: `push_line` doesn't panic, ring still records.

**Rust — new `src-tauri/tests/phase13_diagnostics.rs`** (integration,
thread-local dispatch — never `set_default`, so parallel tests stay clean):

6. `tracing_events_flow_to_stderr_ring_and_session_file` — build
   `tracing_subscriber::fmt().with_writer(TeeWriter over a standalone hub
   + tempdir session sink).finish()` into a `tracing::Dispatch`, emit
   `tracing::warn!("phase13-marker-<uuid>")` inside
   `tracing::dispatcher::with_default(&dispatch, …)`, assert the marker is in
   both the hub snapshot and the session file.
7. `diagnostics_file_uses_log_extension_so_watcher_ignores_it` — construct
   the session path exactly as Step 3 does (`save_path.with_extension("log")`)
   and assert the extension is `log` (guards the vault-watcher contract by
   construction; `is_markdown` itself is private to `vault.rs`).

**Vitest — new `src/lib/diagnostics.test.ts`** (colocated, pure):

- `formatDiagnosticsMarkdown` includes version/OS/session lines, wraps the
  log lines in the details block, handles `session_id: null` and empty
  `lines`.

**Gates** (sandbox notes): `cd src-tauri && PKG_CONFIG_PATH=/tmp/opencode/gtk-shim/pkgconfig cargo test`
(default) and `cargo test --all-features` 0-fail; `~/.nvm/versions/node/v24.12.0/bin/pnpm test`;
`pnpm check` (0 errors / 0 warnings); `pnpm build`. No env-gating — no
network, no GUI. `PF2E_NET_TESTS`/`PF2E_NET_WAN`/`PF2E_GUI_SMOKE` untouched.

## 4. Acceptance criteria

1. With the app running, `/settings` (new topbar pill) shows the Diagnostics
   card; **Copy diagnostics** puts a paste-ready markdown blob on the
   clipboard (version, OS, arch, active session, last ≤2,000 log lines); the
   `<details>` textarea fallback shows the same text.
2. Console/stderr output is unchanged from before the slice (same lines, same
   filter defaults).
3. Hosting or resuming a session creates/appends
   `<vault>/campaigns/<campaign>/sessions/<session-id>.log` with a start
   marker and subsequent log lines; ending/leaving/re-hosting stops writes to
   the old file and starts the new one.
4. The `.log` file never appears in search results or `/vault` (extension
   filter — verified, no watcher change needed).
5. The diagnostics blob contains **no secrets**: no table PIN, no iroh p2p
   ticket, no LLM API key (none are traced today — this is a review
   checkpoint, see Risks).
6. Ring is bounded: memory footprint of captured logs never exceeds the cap
   (~400 KB) regardless of session length.
7. All gates green: `cargo test` (+`--all-features`), `pnpm test`, `pnpm
   check` (0/0), `pnpm build`. No lockfile changes.

## 5. Risks / unknowns

- **Secret-leak guardrail (the one real risk)**: today nothing traces the
  PIN, p2p ticket, or API key (verified: `lan.rs` logs addrs/errors,
  `net.rs` errors, `commands.rs` lag counts) — but diagnostics makes any
  future careless `tracing::info!(pin)` instantly pasteable. Reviewer must
  diff-check new `tracing` calls in this slice for secrets; consider a
  follow-up grep-CI rule (`tracing::.*pin|ticket|api_key`) later.
- **`MakeWriter` contract**: the fmt layer may deliver one line in several
  `write` calls — the buffering-until-`\n` design is mandatory (test 2
  covers it). `io::Write::flush` on the handle is a no-op beyond the
  buffer; the global subscriber handles flush ordering.
- **Global subscriber only exists in `run()`** — tests must never install a
  global default (parallel-test flakiness); the plan's integration test uses
  `tracing::dispatcher::with_default` (thread-local) with its own hub. Do
  not "simplify" this away.
- **Session file growth**: append-mode means a long-lived resumed session's
  `.log` grows unboundedly across resumes. Acceptable for beta (info-level
  volume is small); if it ever matters, truncate on resume and accept the
  loss — note in the module doc, decide later.
- **`host()`/`load()` line drift**: service.rs changed in Phase 12 (export
  plumbing) — line numbers cited here are from the post-`02acd14` read
  (host ~491/save_path ~514, load ~595, shutdown 124); implementer should
  locate by symbol, not line.
- **Peer joins** log to the ring only (no save path) — deliberate; the GM's
  host process is the one that files bug reports.
- **No new deps**: `OnceLock` is std; `VecDeque` is std; `MakeWriter` comes
  from the already-pinned `tracing-subscriber` 0.3 (`env-filter` feature
  already on). `Cargo.lock`/`pnpm-lock.yaml` must not change.
- **Mobile**: clipboard + textarea fallback both work in the Tauri webview;
  no plugin involved. Browser LAN clients never call `diagnostics_get`
  (no Tauri IPC) — acceptable, the GM/host is the diagnostics source.
