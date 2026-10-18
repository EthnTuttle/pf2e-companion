# PLAN — F-R1: lobby LAN join (post-real-device fix)

Status: IMPLEMENTED. Source of truth: `docs/ui-reviews/2026-10-18-real-device.md`
§F-R1 (the top blocker from the 2026-10-18 real-device evening) +
`docs/plans/PLAN-TIER1-deep-links.md` (whose "prefill only; the joiner still
presses **Join Session**" non-goal made this button the deep-link value
proposition's single point of failure).

**Root cause (one sentence).** The lobby's Join submit unconditionally invoked
`session_join_lan`, a Tauri command that was never registered in
`src-tauri/src/lib.rs` — it only ever existed as an e2e mock fixture — and the
deterministic lane mocked IPC while the live lane bypassed the button via URL
params, so no harness ever exercised a real click.

Exact failure strings on device (`ui-reviews` §F-R1):

- Browser (Vanadium, typed lobby entry): `TypeError: Cannot read properties of
  undefined (reading 'invoke')`
- Native (Pixel 7a APK, deep-link-prefilled lobby): `Command session_join_lan
  not found`

## 1. What & why

**What.** The lobby's LAN Join path is now transport-pure: the button hands the
typed name/host/PIN to the play surface as `/table/play?host=…&pin=…&name=…`
URL params (the proven direct-URL flow), and `createTransport` now lets an
explicit `?host=` target win over the injected Tauri internals so the native
webview — which can open `ws://` directly — uses the same `wsTransport` a
browser does. `session_join_lan` is gone from the codebase, mock included.

**Why (decision vs. the device report's recommended candidate).** Two options
were on the table:

1. *Implement the Rust command* — a WS-client actor in src-tauri bridging
   frames onto the `session:patch`/`session:presence` Tauri events.
2. *Remove the dead IPC path* — route lobby Join through `wsTransport` in all
   shells.

Candidate 2 was chosen, on this evidence:

- **The wsTransport path was the only one already proven on hardware.** A4
  (direct `/table/play?host=…` from the phone browser) and C1 (real desktop
  host ↔ real phone over true Wi-Fi) both passed on device, and L1–L4 pin it
  live. Candidate 1 would have shipped a brand-new, device-untested actor.
- **No shell capability gap blocks it.** `tauri.conf.json` CSP is `null` (no
  connect-src restriction in the webview) and the Android manifest ships
  `usesCleartextTraffic=true` (ui-reviews F-R11), so the native shell can open
  the host's LAN socket exactly like the phone browser did.
- **The session registry is the wrong shape for it.** `ActiveSession` /
  `SessionHandle` structurally wrap a local `SharedDoc` service
  (`session/service.rs`); a LAN-joined thin client has no local doc — intents
  must forward over the wire, not apply locally. Candidate 1's honest version
  is a second handle mode plus a WS client, not a small additive command.
- **Parity by construction.** With candidate 2 there is exactly one LAN-join
  code path; browser, native, and deep-link-prefill entries converge on the
  same URL-param surface the live lane already exercises. Candidate 1 would
  have kept browser and native on divergent implementations — the structural
  setup that let F-R1 hide.
- **Failure UX improves for free.** Wrong/missing PIN now flows through the
  play page's existing error-banner + redirect-to-lobby path (L2-pinned,
  "bad or missing table PIN"), satisfying the report's "readable message, not
  a raw TypeError" bonus. Non-`ws(s)` URLs are rejected inline in the lobby
  before any navigation.

**Non-goals.** Outbound intents from a WS-joined page (`wsTransport.send` has
no caller; the play page's send seam is the ipc.ts `session_intent` invoke)
stay exactly as they were — that is the documented F-R7 seam, owned by a
different fix batch (ui-reviews §Minor). LAN reattach + PIN semantics
(CONTRACT-10_5 § 1) are untouched. `session_join_peer` (iroh tickets) is
untouched.

## 2. Implementation

Paths relative to repo root `pf2e-companion/`.

- `src/lib/transport.ts` — `createTransport`: an explicit `?host=` query param
  now wins in EVERY shell (checked before `__TAURI_INTERNALS__`); bare-browser
  with neither still throws. This is the entire native-side enabler: host /
  ticket-join / resume flows navigate to `/table/play` bare and keep landing
  on `ipcTransport`; only a LAN join carries `?host=`.
- `src/routes/table/+page.svelte` — lobby `handleJoin` LAN branch only
  (line-level): validate the URL (`ws:`/`wss:` + a session-id path; tolerate
  pasted `/ws/<id>` wire form and one trailing slash), normalize the PIN
  (trim + uppercase, mirroring the removed call site), then
  `goto(/table/play?name=…&host=…&pin=…)` and return — the ticket branch and
  the shared `sessionState` tail are byte-unchanged. No `invoke` import
  remains in the handler.
- `e2e/fixtures.ts` — removed the `session_join_lan` mock entry, the fantasy
  stub that pinned the fantasy command.
- `docs/plans/PLAN-TIER1-ui-shots.md` — stub-list row updated to match.

### PIN-in-query posture (relitigated, narrowly)

Phase 10.5's "never put `?pin=` in URLs" targeted *browser history* leaks.
This change makes the lobby navigate with the PIN in the query — the same
exposure class as the live lane's `playerUrl` and the device's proven A4
workaround, and the play URL is where the reconnect machinery (`?pin=` re-read
on retry) gets its credentials. The prefill scrub (lobby `onMount`,
`replaceState`) still guarantees a deep-linked PIN never lingers at the
lobby's own history entry. No new persistence channel was introduced.

## 3. Tests

- **Live lane (`PF2E_E2E_LAN=1`), `tests/e2e/live-lan.spec.ts` — L5
  join-via-lobby-button (new):** real `e2e_host`, real button clicks, run with
  `tauri: true` (the shell class that failed natively; pins the
  `?host=`-first rule end-to-end). Leg 1 drives typed entry (lowercase PIN →
  normalized); leg 2 drives a deep-link-prefilled lobby
  (`/table?join=1&url=…&pin=…`). Both assert the L1 join end state (h1 adopts
  the live session name, Player badge, no error banners), leg 1 adds the
  roster broadcast + one GM-chat receive leg, leg 2 re-asserts end state and
  roster stability after the same-name reattach.
- **Deterministic lane, `tests/e2e/table-lobby-lan.spec.ts` (new, 2 tests):**
  the navigation contract (host/pin/name params; `/ws/`-prefix + trailing-slash
  + lowercase-PIN normalization) with an assertion that NO session
  invoke beyond the boot-time `session_list_saved` was recorded — the
  anti-fantasy-IPC pin that would have caught F-R1 — plus the inline non-`ws`
  rejection (readable message, no navigation, no IPC).
- **Vitest, `src/lib/transport.test.ts` (+1):** explicit `?host=` beats
  injected Tauri internals.
- Gates: `cargo test --all-features` (unchanged surface — no Rust diff),
  `pnpm check` 0/0, `pnpm test` (+1), `pnpm build`, `pnpm e2e` (+2),
  `pnpm e2e:live` (+1).

## 4. Acceptance criteria

1. Browser lobby → typed URL+PIN → Join reaches the identical session state
   as a direct `/table/play?…` URL (L5 leg 1 + deterministics).
2. Native deep link → prefilled lobby → Join reaches the same state (L5 leg 2,
   run with Tauri internals present).
3. Wrong/missing PIN degrades to the play page's readable banner + lobby
   redirect (L2, unchanged and now on the join path).
4. `session_join_lan` appears nowhere in the repo's code or stubs.
5. All gates in §3 green, no retries.

## 5. Risks / follow-ups

- **F-R7 seam unchanged.** A WS-joined page (browser or, now, native) still
  cannot send intents — `wsTransport.send` has no caller. Phone→GM chat was a
  dead end before this fix and remains one; it is queued separately.
- **Harness BASE_RESPONSES audit (per the task).** `tests/e2e/helpers/
  harness.ts` carries `list_lenses`, `list_campaigns`, `schema_version`,
  `llm_status`, `session_list_saved` — all five exist in `lib.rs`'s handler
  list; nothing to remove there. The only fantasy stub repo-wide was
  `session_join_lan` in `e2e/fixtures.ts` (removed). `check_update` appears
  nowhere in `src/`, `src-tauri/`, `e2e/`, or `tests/` — neither registered
  nor mocked. Plugin-scoped `plugin:dialog|save|open` stubs resolve against
  tauri-plugin-dialog at runtime and are legitimate.
- **`session_snapshot` / `wizard_validate_vault` have no deterministic-lane
  stub** — deliberate (fail-fast on first use beats a pinned fantasy); noted
  so nobody "fixes" the asymmetry by adding unexercised stubs.
- **Invite panel after a LAN join.** `myLanUrl`/`myTicket` are unset on this
  path (no `SessionInfo` is minted locally), so the invite panel correctly
  stays hidden for joiners — unchanged from the ticket-join flow.
