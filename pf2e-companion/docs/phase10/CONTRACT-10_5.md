# Phase 10.5 — Hardening Addendum (FROZEN)

Extends `docs/phase10/CONTRACT.md` (still frozen; this file wins where they differ).
Scope = exactly the five items below. Doc schema stays **v1** (additive keys only;
PIN lives outside the automerge doc). No new cargo or npm deps.

## 1. LAN join PIN (fixes known-limitation 1)

- `session::service::gen_pin()` — 6 chars from `Uuid::new_v4()` bytes
  (`u16 % 30`, same entropy style as dice.rs):
```rust
pub const PIN_ALPHABET: &[u8; 30] = b"23456789ABCDEFGHJKMNPQRSTVWXYZ";
pub fn gen_pin() -> String; // 6 chars, uppercase
```
- Rust: `SessionInfo` gains `pub pin: Option<String>` (host: Some; joiner/load: from stored `ActiveSession.pin`). `ActiveSession` gains `pin: Option<String>` — in-memory only, NEVER in the doc/snapshot. `SessionService` gains `pin: Option<String>` (set at host; `session_load` re-mints one).
- Wire: `ClientFrame::Hello { session_id, name, #[serde(default)] pin: Option<String> }`. All other frames byte-identical.
- Server (lan.rs): if `svc.pin.is_some()`, hello with wrong/missing `pin` → `{"type":"error","message":"bad or missing table PIN"}` + close. Applies equally to first join AND same-name reattach (pin+name required to reattach). Sessions without a pin accept any hello (back-compat).
- Tauri IPC paths (`session_host`/`session_join_peer`) are local-trusted — no pin. iroh path unchanged (ticket possession = auth). Full identity signatures → icebox.
- TS: `SessionInfo.pin?: string|null`; `wsTransport(lanUrl, name, pin)` gains 3rd param (also read from `?pin=` query param).

## 2. Re-host port race (fixes limitation 2)

Ordering fix, exact deltas:
```rust
// session/service.rs
impl SessionService {
    /// Signal + abort + AWAIT all registered tasks (frees ports before return).
    pub async fn shutdown_and_wait(&self) {
        let _ = self.shutdown.send(true);
        let tasks = std::mem::take(&mut *self.tasks.lock().expect("task registry"));
        for t in &tasks { t.abort(); }
        for t in tasks { let _ = t.await; } // JoinHandle resolves after abort
    }
}
impl SessionRegistry {
    /// Take + persist + shut down the old session IN FULL before returning.
    pub async fn take_and_shutdown(&self) -> Option<ActiveSession>;
}
```
- commands.rs: `session_host`, `session_load`, `session_join_peer` call
  `registry.take_and_shutdown().await` FIRST, then `host()/load()/join_peer()`,
  then `install_session`. `install`'s internal old-session branch stays as a
  no-op safety net.

## 3. Chat dedupe `seq` (fixes limitation 3)

- Doc: `meta.next_chat_seq: i64` (init 1). `append_chat` stamps `seq` and
  increments the counter. `ChatEntry` gains `#[serde(default)] pub seq: Option<u64>`
  (None only for pre-10.5 entries in legacy saves). Snapshot reader maps it back.
- TS: `ChatEntry.seq?: number`.
- Reducer (session.svelte.ts `chat_appended`):
```ts
const dup = patch.entry.seq != null
  ? snap.chat.some((e) => e.seq === patch.entry.seq)
  : snap.chat.some((e) => e.seq == null && e.ts === patch.entry.ts
                       && e.from === patch.entry.from && e.text === patch.entry.text);
if (!dup) { snap.chat.push(patch.entry); snap.chat.sort((a,b)=>(a.seq??a.ts)-(b.seq??b.ts)); }
```

## 4. Presence UX completeness

- lan.rs: per-socket `tokio::time::interval(15s)` arm in the select loop sends
  tungstenite `Message::Ping`; browsers auto-pong. (Server already answers client pings.)
- Store: `sessionState.connection: "offline"|"connecting"|"connected"|"reconnecting"|"failed"`,
  plus `sessionState.reconnectAttempt: number`. New `retryConnection()` in session.svelte.ts.
- transport.ts: `wsTransport` gains optional `onStateChange(cb: (s) => void)`;
  auto-reconnect on unexpected close using stored `{lanUrl, pin, name}`:

| from | event | to | action |
|---|---|---|---|
| connected | ws close (not manual) | reconnecting | retry now |
| reconnecting | connect ok | connected | hello(w/ pin) → welcome → snapshot |
| reconnecting | fail | reconnecting | backoff 500ms→1s→2s, attempt++ |
| reconnecting | attempt > 3 | failed | onError("connection lost") |
| failed | Rejoin clicked | connecting | fresh `connect()` |

- UI (table routes, WEB): lobby shows GM's PIN copyable; join form gains PIN input;
  connection banner states: `reconnecting` → "Reconnecting… (attempt n/3)";
  `failed` → error banner + [Rejoin] button (`retryConnection()`).
  Presence list dims players with `connected === false`.
- ipc.ts (additive, fixes audit note 4): PlayerInfo + `player_upserted.player` gain
  `connected?: boolean` (absent-safe; treat absent as `true`).

## 5. A11y / svelte-check cleanup

`pnpm check` must end with **0 errors** and warnings ONLY in
`src/lib/EntityEditor.svelte` (10 pre-existing, out of scope). All 9 warnings in the
Phase-10-created files (`src/routes/table/+page.svelte`, `src/routes/table/play/+page.svelte`)
go to zero. No new warnings introduced anywhere.

## 6. Test plan additions

Rust (`src-tauri/tests/`):
- `phase10_lan.rs` += `hello_with_wrong_pin_is_rejected_and_closed`,
  `hello_without_pin_is_rejected_and_closed`, `reattach_requires_matching_pin`,
  and `session_without_pin_accepts_any_hello` (back-compat, via SessionService::new with pin None).
- `phase10_table_demo.rs` += `rehost_in_process_reuses_port_4780`: host A →
  take_and_shutdown → host B; assert B's `lan_url` ends with `:4780/…` and a WS
  client completes hello on it.
- `phase10_session.rs` += `chat_seq_monotonic_and_survives_save_load`
  (two rolls same tick keep both; reload keeps seqs).
Web (`src/`, vitest):
- `session.svelte.test.ts` += seq-dedupe (same seq dropped), legacy-key fallback,
  mixed seq/legacy coexistence.
- `transport.test.ts` += `wsTransport` reconnect with `vi.useFakeTimers()` + fake
  WebSocket: close→reconnecting→success restores connected; 3 failures → failed +
  onError fired; hello frame includes `pin` when provided.

## 7. Ownership (unchanged from CONTRACT.md §7)

- **BUILDER-RUST**: `src-tauri/src/**`, `src-tauri/tests/**` (+ Cargo.lock refresh only if
  versions drift; no new deps allowed).
- **BUILDER-WEB**: `src/**` (incl. ipc.ts/transport.ts/session.svelte.ts, table routes).
- Frozen: `docs/phase10/CONTRACT.md`, this file (amendments via reviewer only).

## 8. Guardrails

- No new deps anywhere (PIN = uuid-derived). `rand` stays forbidden.
- Wire-compat: only additive serde fields with `#[serde(default)]`; v1 docs load unchanged.
- Gates: `cargo test` (default) + `cargo test --all-features` 0-fail;
  `PF2E_NET_TESTS=1 cargo test --test phase10_p2p` still 2/2 (cached loopback);
  `pnpm test` and `pnpm check` per §5. Never `pnpm tauri build`.
- Keep the IPC/WS type names byte-identical to CONTRACT.md §3 (plus the additive
  fields defined here); both sides copy from THIS file for 10.5 deltas.
