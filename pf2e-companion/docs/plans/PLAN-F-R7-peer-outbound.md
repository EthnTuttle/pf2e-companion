# PLAN — F-R7: phone→GM outbound seam over wsTransport (peer intents)

Status: IMPLEMENTED. Source of truth: `docs/ui-reviews/2026-10-18-real-device.md`
§F-R7 + `docs/plans/PLAN-F-R1-join-lan.md` §5 (the seam record) +
`tests/e2e/live-lan.spec.ts` header note (the harness-level confession).

**Root cause (one sentence).** The play page sends EVERY intent through
`ipc.ts`'s `sessionIntent` Tauri command instead of through its own
`createTransport` handle, so on a ws-joined page (`?host=…`, browser or —
post-F-R1 — native webview) the send never reaches the wire: in a plain browser
the invoke throws on absent `__TAURI_INTERNALS__`; in a native LAN-join webview
it hits a local Rust registry that holds no session for this page. The wire
protocol and host authority for peer intents existed and were already pinned —
the dead hop is purely the frontend's send seam.

Quoted evidence:

- Device evening, `docs/ui-reviews/2026-10-18-real-device.md:165`:
  > **F-R7** Phone→GM chat is a known dead end: phone sends, nothing reaches
  > the host log (matches the header note in `tests/e2e/live-lan.spec.ts`).
  > GM→phone works.
  Verdict row A6: "Phone→GM chat | dead end (known seam, F-R7)".
- The seam record, `docs/plans/PLAN-F-R1-join-lan.md:65` (non-goals):
  > Outbound intents from a WS-joined page (`wsTransport.send` has no caller;
  > the play page's send seam is the ipc.ts `session_intent` invoke) stay
  > exactly as they were — that is the documented F-R7 seam.
- The code: `src/routes/table/play/+page.svelte` — every send is a bare
  `sessionIntent({ kind: "chat", … } as Intent)` (line 57; 15 call sites),
  imported from `$lib/ipc` at line 5, bypassing the `transport` the page's own
  `init()` created at line 286.
- The live-lane confession, `tests/e2e/live-lan.spec.ts` header:
  > the play page's outbound sessionIntent is the ipc.ts invoke — on a
  > wsTransport page that seam is UNWIRED today (wsTransport.send has no
  > caller …)

**Negative evidence (what is NOT broken — checked before touching the wire):**

- A peer→host frame type EXISTS: `src-tauri/src/lan.rs:38`,
  `Intent { intent: Intent }` in `ClientFrame`, shaped exactly like what
  `wsTransport.send` already serializes (`{type:"intent", intent}` —
  `transport.ts:211`).
- The bridge APPLIES peer frames with role gating: `lan.rs:407-422` parses
  `ClientFrame::Intent` and calls `svc.apply(&peer_id, Role::Player, intent)`,
  routing rejections back to the SENDER as an `error` frame.
- The authority enforces the trust model: `session/mod.rs:297` `authorize()`
  lets a Player run `chat` / `roll_dice` / `create_character` / own-sheet
  `update_sheet` / `move_token` and answers every GM-only intent with
  `forbidden: this intent is reserved for the GM`.
- Rust coverage pins all of it: `phase10_lan.rs:145` proves a ws client's chat
  broadcasts to every client; `phase10_lan.rs:176` proves a player's
  `set_initiative` gets an error frame and "Host state untouched".

## 1. What & why

**What.** The play page now routes every outbound intent through its transport
handle (`transport.send`) — the seam `createTransport` exists for, and the one
`ipcTransport.send` was already implementing by calling the same
`session_intent` invoke. One local `sendIntent` wrapper replaces all 15 direct
`sessionIntent(...)` call sites; GM/desktop behavior is byte-identical at the
wire level (ipcTransport.send IS sessionIntent), while ws-joined pages'
intents now serialize as §4.3 `{"type":"intent","intent":…}` frames the host
has always accepted. A ws-side authority rejection already reached the DOM via
`transport.onError` → `sessionState.error` — it simply never had traffic
before.

**Wire-compat decision (explicit, per the fix batch's compat posture).** NO
protocol change — additive-or-nothing analysis lands on "nothing": the
smallest possible fix is using the frame type the bridge has accepted since
phase10. Mixed-version pairings degrade gracefully in BOTH directions:

- New player page → old host: old hosts already parse `type:"intent"`
  (`ClientFrame::Intent` predates this fix), so the pairing WORKS, not breaks.
- Old player page → new host: identical — nothing on the host changed; the old
  page keeps its dead seam (the bug we are fixing) but cannot break the host.

No capability detection is needed: there is no new frame to gate.

**Why transport-routing, not a local-branch in sendChat.** The phase10
architecture sentence is "Play UI only knows intents + patch events"
(`docs/phase10/PLAN.md:38-40`, the transport-seam bullet). Making ONE caller
honest (this page) instead of teaching the composer about ws restores that
design for every intent kind at once — chat, rolls, sheet edits, token drags —
with one wrapper.

**Non-goals.** Lobby markup/CSS and `+layout.svelte` (owned by F-R3/F-R6);
host-advertise logic (F-R2 landed); `session:snapshot` doc reconciliation;
`sessionState.info` on ws pages stays null (role badge defaults Player — the
bridge only ever mints Players, so this is truthful); p2p/iroh peer path.

## 2. Implementation

Paths relative to repo root `pf2e-companion/`.

- `src/routes/table/play/+page.svelte` — script-only. The `$lib/ipc` value
  import of `sessionIntent` is replaced by `createSendIntent` from
  `$lib/session.svelte` (its first caller — the helper was written for this
  seam and clears `sessionState.error` before each send). One wrapper:

  ```ts
  // F-R7: outbound intents ride the transport, not ipc.ts directly — on a
  // ws-joined page there IS no session_intent IPC behind this session.
  function sendIntent(intent: Intent): void {
    const t = transport;
    if (!t) return;
    void createSendIntent(t)(intent).catch((e: unknown) => {
      sessionState.error = e instanceof Error ? e.message : String(e);
    });
  }
  ```

  All 15 call sites swap `sessionIntent(… as Intent)` → `sendIntent(… as
  Intent)`, keeping fire-and-forget UX. ipcTransport pages record the identical
  `session_intent` mock IPC as before; transport-level failures (socket not
  OPEN) land in the same `sessionState.error` banner the bridge's `error`
  frames already used. Markup/styles untouched.

  **Late-breaking second hop the wrapper exposed (gate-caught, see §5):** on
  the lobby-first flows (host / ticket join / resume) the play page's `init()`
  early-returns (`sessionState.connected && info` already set by the lobby),
  so its local `transport` stays `null` — the lobby creates and connects the
  transport, then navigates here. Pre-fix nothing noticed: sends bypassed
  transports entirely (the bypass WAS the bug). Two supporting edits make the
  adoption honest.

- `src/lib/session.svelte.ts` — `sessionState.transport` + `setTransport`
  widened from the retry/close structural subset to the full `Transport`
  (type-only import, no cycle), and `createSendIntent` retyped `unknown` →
  `Intent` (`unknown` was unassignable from `Transport.send` under
  property-style strictness).
- `src/routes/table/+page.svelte` — logic-only, no markup/CSS: `handleHost`,
  `handleJoin`'s ticket branch and `handleResume` now `setTransport(transport)`
  after connecting, so the play page's early-return `init()` can adopt
  `sessionState.transport` and send on the SAME backend the patches arrive
  on (the ws direct-URL flow is unchanged — that page builds its own).

- `src-tauri/examples/e2e_host.rs` (test-only, feature `lan`) — the live
  GM-side observation tap: after boot, a broadcast subscriber mirrors every
  service broadcast patch to stdout as `{"type":"bcast","event":{…}}` — a NEW
  NDJSON line type, additive to the harness protocol (`patch`/`applied` lines
  untouched; existing `gmIntent` collectors ignore `bcast`). This is what lets
  the live lane assert "the GM side received the peer's chat" — the e2e_host
  process IS the GM surface in that lane.
- `tests/e2e/server/liveHost.ts` — `LiveHost.waitForBroadcast(pred, what):
  Promise<LivePatchEvent>`, a bounded await on `bcast` lines (`awaitPatch`
  semantics borrowed from `gmIntent`'s collector).
- `tests/e2e/harness/global.d.ts` + NEW `tests/e2e/helpers/wsBridge.ts` — the
  deterministic lane's ws-level mock, installed harness-style via
  `page.addInitScript`: a `MockWebSocket` class that plays open/handshake
  timing truthfully (async `onopen`), records every sent frame's raw JSON on
  `window.__pf2eWs.sent`, and exposes `deliver(frame)` for the test-as-
  authority to push host→client frames through the page's REAL wsTransport
  parse path (`onmessage`).

## 3. Tests

- **Deterministic lane, `tests/e2e/table-play-lan.spec.ts` (new, 3 tests):**
  two-context rig — a real GM page (harness IPC, `hostViaLobby` driver) and a
  PLAYER page on the mocked wsTransport (`tauri:false` + `installWsMock`,
  handed §4.3 welcome/snapshot frames exactly as `HostFrame` serializes them).
  1. *player chat reaches the wire (not the mock registry)*: typed text +
     Enter produces exactly one sent frame `{"type":"intent","intent":{kind:
     "chat", text:…}}` and ZERO `session_intent` invocations (the anti-IPC pin
     that would have caught F-R7); the test then plays authority — relays the
     intent as a `chat_appended` patch to the GM page via `emitPatch` and back
     to the player via `deliver` — and both DOMs render the line once.
  2. */roll rides the same seam* (`roll_dice`, `secret` honored).
  3. *authority rejection renders*: a delivered `error` frame
     (`forbidden: this intent is reserved for the GM`, rejected
     `set_initiative`) lands in `.session-error-banner` — the DOM surface a
     player's rejected state intent produces.
- **Live lane, `tests/e2e/live-lan.spec.ts` — L6 peer-outbound-chat (new):**
  `@live`, `joinPlayer` then real Enter in the composer; assert (a)
  `host.waitForBroadcast` sees `chat_appended` with the player's text —
  the GM authority applied a PEER frame; (b) the player's own chat-log echoes
  it (broadcast round-trip through the real bridge); (c) role gate: a raw
  in-page WebSocket joins as "Rebel" and sends `set_initiative` — the real
  bridge answers an `error` frame containing "GM" with
  `rejected_intent.kind === "set_initiative"`, and NO broadcast of it reaches
  the roster/GM tap; (d) evidence shot of the rendered echo.
- **Live lane lockstep edits forced by the fix** (previously pinned the dead
  seam as expected-broken, now pinned as fixed): the header note and L1's
  "send a chat" adaptation are rewritten to the real round trip (Enter → the
  host's broadcast echoes the line on the sender's DOM; `expectIntents`
  mock-log shape assertion and the pre-GM-apply `toHaveCount(0)` removed);
  `joinPlayer` drops the `session_intent: {ok:null}` stub — an IPC fall-back
  now fails loudly under the harness instead of being silently absorbed
  (fail-fast beats pinned fantasy, per the F-R1 harness-audit note).
- **Vitest, `src/lib/transport.test.ts` (+1):** `wsTransport.send` actually
  serializes `{type:"intent", intent}` on the socket (the existing
  "intent frame structure" test built the shape by hand and never called
  `send`).
- Gates: `cargo test --all-features` (example-only Rust touch + new example
  code), `pnpm check` 0/0, `pnpm test` (132+1), `pnpm build`, `pnpm e2e`
  (29+3), `pnpm e2e:live` (6/6).

## 4. Acceptance criteria

1. A LAN-joined player (browser path AND native-webview path — now the same
   `createTransport` branch per F-R1) sends chat from the composer and every
   surface (GM process tap live-lane-verified; both DOMs deterministic-
   verified) renders it once.
2. No intent leaves a ws-joined page via `session_intent` (deterministic
   zero-invocation pin + live-lane loud-failure posture).
3. Players still cannot apply state intents: the real bridge's
   `authorize(Role::Player, …)` rejection rides home as an `error` frame and
   renders — pinned live at the wire (L6 leg c) and deterministic at the DOM
   (table-play-lan leg 3). Rust's `phase10_lan.rs` pins remain untouched.
4. Zero wire-protocol change (ClientFrame/HostFrame byte-identical) — a
   mixed-version pair cannot notice this fix except by the seam starting to
   work.
5. All gates in §3 green, no retries.

## 5. Risks / follow-ups

- **Reconnect re-handshake**: unchanged from F-R1 — wsTransport re-`hello`s on
  reconnect and the bridge re-`peer_joined`s; outbound send during
  reconnection throws "WebSocket not connected" into `sessionState.error`
  (visible, dismissible) instead of failing silently. That mirrors the
  pre-existing reconnect UX; composing-during-reconnect polish is out of
  scope.
- **`.session-error-banner` on send failure is now reachable in more cases**
  (transport throws). This is the intended surface (L2 already drives it).
- **The GM badge on ws pages** remains "Player" for everyone — truthful (the
  bridge mints only Players); a "GM over LAN" mode would need bridge-side
  role promotion and is deliberately not this fix.
- **L1 update scope**: L1's send leg moved from mock-log assertion to the
  real echo; its roster/snapshot assertions are untouched, and L6 owns the
  GM-side pickup + role-gate legs so L5's comment trail stays accurate.
