# PLAN — Tier 1 slice: "Session invite deep-links" (SHIPPING.md item 2)

Status: IMPLEMENTED. Source of truth: `docs/SHIPPING.md` § Tier 1 item 2 —
"register `pf2e://join?url=…&pin=…` scheme + universal-link/App-Intent so
'text your friend a link' replaces typing host:port+PIN. (iroh tickets remain
for desktop-desktop.)"

## 1. What & why

**What.** A `pf2e://join?url=<ws-url>&pin=<pin>` deep link (or
`pf2e://join?ticket=…` for desktop-desktop P2P) opens the app and prefills
the lobby's join form. The host gets a **Copy invite link** action next to
the existing ticket/URL/PIN copy buttons.

**Why.** The friends-beta join ritual today is read-aloud host:port + PIN
over voice chat, or a `ws://` URL texted bare. A custom-scheme link turns it
into: text link → friend taps → app opens prefilled → type name → Join.

**PIN posture (relitigated).** Phase 10.5's "never put `?pin=` in URLs"
targeted *browser history* leaks. A `pf2e://` link is opened by the OS in the
app — no browser history is involved — so `pin=` is acceptable in the scheme.
Unchanged: the LAN/mobile-browser URL path (`?host=` page param) still
expects a user-typed PIN; this slice adds no browser URL carrying a PIN.

**Non-goals.** No universal-link host (`https://` app links need a server
with `.well-known` files — none exists; Tier 2 may add one). No
single-instance plugin (see Risks). No auto-join — prefill only; the joiner
still types their name and presses **Join Session**.

## 2. Implementation

Paths relative to repo root `pf2e-companion/`.

### Rust / config

- `src-tauri/Cargo.toml`: `tauri-plugin-deep-link = "2"` (repo toolchain
  1.93 ≥ the plugin's 1.90 MSRV).
- `src-tauri/src/lib.rs`: `.plugin(tauri_plugin_deep_link::init())` plus, in
  `setup`, `app.deep_link().register_all()` gated
  `#[cfg(any(target_os = "linux", all(debug_assertions, windows)))]` — the
  plugin docs' recommended scope so `tauri dev` and Linux AppImages (which
  skip the install-time registration) still resolve `pf2e://`. Warn-only on
  failure; scheme registration must never gate app boot.
- `src-tauri/tauri.conf.json`: `plugins.deep-link.desktop.schemes: ["pf2e"]`
  + `plugins.deep-link.mobile: [{ scheme: ["pf2e"], appLink: false }]` —
  the `appLink: false` shape is the plugin's custom-scheme (no-host, no
  `.well-known` verification) mode; the plugin's build script generates the
  Android intent filter and iOS `CFBundleURLTypes` from it.
- `src-tauri/capabilities/default.json`: `deep-link:default` (permits
  `get_current`; event listening is already covered by `core:default`).

### Frontend

- `src/lib/deeplink.ts` (new, pure except the last function):
  - `parseInviteUri(href)` — strict `pf2e` scheme, `join` command,
    `url` must be `ws(s)://` with a host, ticket structurally plausible
    (16…4096 chars, no whitespace), PIN normalized uppercase against the
    Crockford-ish alphabet from `session/service.rs`; a malformed PIN
    degrades to `null` (invite survives, user retypes) rather than sinking
    the invite. `pin=` on a ticket invite is dropped (IPC peers carry none).
  - `buildInviteUri(invite)` — inverse; `null` when there's nothing valid to
    share.
  - `inviteToLobbyQuery` / `inviteFromLobbyQuery` — the `/table?join=1&…`
    mirror the app navigates to. The `join=1` marker makes prefill
    intentional (a stray `?url=` on the lobby URL never touches the form).
  - `initDeepLinkHandling(onInvite)` — the only impure bit: dynamic-imports
    `@tauri-apps/plugin-deep-link`, delivers cold-start `getCurrent()` and
    hot `onOpenUrl()` URLs through `parseInviteUri` into the callback.
    Returns whether a *cold-start* invite was handled. Everything is
    swallow-wrapped so plain browsers and the e2e IPC stub (which rejects
    unknown commands) are unaffected.
- `src/routes/+layout.svelte`: `onMount` calls `initDeepLinkHandling(invite
  → goto("/table?" + inviteToLobbyQuery(invite)))` before the wizard check;
  a cold-start invite skips the first-run redirect (joining a friend's table
  is why the app just opened; the lobby needs no campaign to join).
- `src/routes/table/+page.svelte` (lobby):
  - `onMount`: `inviteFromLobbyQuery(page.url.searchParams)` prefills
    `joinMode` + `lanHost`/`lanPin` or `joinTicket`, then scrubs the params
    with `goto("/table", { replaceState: true })` so a PIN never lingers in
    the app's navigation history. Join stays manual.
  - Invite panel: **Copy invite link** button (renders when `myLanUrl`
    exists — i.e. right after hosting) built via `buildInviteUri` from the
    exact strings already on screen + `sessionPin`. Per the S14 notes the
    panel is component-local and dies on navigation — known and accepted;
    the share action lives where the strings exist.
- `package.json`: `@tauri-apps/plugin-deep-link ^2`.

## 3. Tests

- **Vitest, `src/lib/deeplink.test.ts` (new, 19 tests):** LAN/ticket parse
  happy paths, PIN normalization (lowercase → upper), malformed-PIN
  degradation, `http(s)` `url=` rejection (LAN invites are `ws(s)` only),
  wrong-scheme/wrong-command/garbage rejection, implausible tickets,
  ticket-invite pin drop, LAN-wins-over-stray-ticket, build↔parse
  round-trips, lobby-query round-trips, `join=1` marker gate.
- **No e2e for the plugin event itself.** `getCurrent`/`onOpenUrl` are
  desktop-shell plugin plumbing; the deterministic harness stubs IPC and
  rejects unknown commands, which `initDeepLinkHandling` treats as no-op —
  prefill and share-side button are covered indirectly by `pnpm e2e` staying
  green (S13–S15 assert the lobby contract; the invite panel remains gated
  on `sessionState.info && connected`).
- Gates: `cargo test --all-features` (src-tauri), `pnpm check` (0/0),
  `pnpm test` (+19), `pnpm build`, `pnpm e2e`.

## 4. Acceptance criteria

1. `pf2e://join?url=ws://…&pin=ABC123` (cold **or** while running) opens the
   app on `/table` with join mode = LAN, host URL + PIN prefilled, and the
   URL scrubbed back to `/table`.
2. `pf2e://join?ticket=…` prefills the ticket tab.
3. Host flow shows **Copy invite link** whose clipboard payload parses back
   to exactly the on-screen LAN URL + PIN.
4. Non-Tauri browser loads and the e2e harness are untouched (no-op path).
5. `pnpm check` stays 0 errors / 0 warnings; all gates above green.

## 5. Risks / unknowns / real-device evening

- **Real-device verification is Tier-1-#1 evening scope**: cold-start tap on
  Android (`adb shell am start -a android.intent.action.VIEW -d
  pf2e://join?… <id>` suffices for a smoke), iOS (`xcrun simctl openurl
  booted pf2e://…`), and Linux (`xdg-open pf2e://…` against a dev run —
  `register_all` makes that work). Verified from the plugin source
  (build.rs + tauri-plugin's `mobile::update_android_manifest`): the Android
  intent filter and iOS `CFBundleURLTypes` are injected as
  AUTO-GENERATED blocks **when `tauri android|ios dev/build` runs**
  (the CLI sets `TAURI_ANDROID_PROJECT_PATH`; a plain desktop
  `cargo build` intentionally skips it). So evening step 0:
  run the mobile build, confirm the injected blocks in
  `gen/android/.../AndroidManifest.xml` / `gen/apple/.../Info.plist`,
  then the tap-through.
- **No single-instance plugin.** On Windows/Linux a tapped link spawns a new
  process delivering the URL as CLI args; the new instance handles it as a
  cold start and the old one keeps running. If the double-window annoyance
  shows up in the friends beta, add `tauri-plugin-single-instance` with its
  `deep-link` feature (the plugin's designed companion) as a follow-up —
  deliberate scope cut here.
- **Scheme squatting — none.** `pf2e://` collision is implausible; worst
  case the parse rejects and the lobby opens blank.
- **PIN in clipboard payloads.** The invite link text contains the PIN —
  same exposure class as the existing bare-PIN copy button. The link never
  enters a URL bar on the wire.
- **Second invite while mid-join** (app open, two taps): each hot event
  navigates to `/table?join=1` again; prefill is idempotent (fields
  overwritten with the newest invite). No state machine to corrupt.
- **Link tapped mid-table** (hot event while `sessionState.connected`):
  ignored — navigating to the lobby would tear down the live session via
  the play page's `onDestroy`. Joining a new table requires leaving the
  current one first. Cold-start has no such guard (nothing to protect).

## 6. Post-device fix (2026-10-18, F-R1)

This slice's non-goal "the joiner still types their name and presses **Join
Session**" made the prefilled lobby's Join button the whole value
proposition — and on the real-device evening that button was dead for LAN
joins (F-R1: `session_join_lan` never existed in src-tauri). Fixed in
`docs/plans/PLAN-F-R1-join-lan.md`: the lobby now hands the typed/prefilled
credentials to `/table/play?host=…` and every shell joins over `wsTransport`.
The prefill → scrub → manual-Join flow designed here is unchanged; what
changed is only what the button does with the fields. Live-lane leg 2 of L5
pins this file's acceptance criterion 1 end-to-end against the real server.
