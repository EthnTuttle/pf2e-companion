# UI Review Findings: native followups — F-R3 root cause + fix, F-R6 fix
**Date:** 2026-10-18
**Findings:** `docs/ui-reviews/2026-10-18-real-device.md` § Major — F-R3, § Minor — F-R6
**Screenshots (post-fix):** `shots/table-lobby/mobile-light.png` + `shots/table-lobby/mobile-dark.png` (pinned Join CTA clear of the tabbar), `shots/table-play-gm/*` (play page unchanged — no new errors)
**Changed files:** `src/routes/table/play/+page.svelte` (F-R3), `src/routes/table/+page.svelte` (F-R6), `tests/e2e/mobile.spec.ts` (F-R6 regression assertion)

## F-R3 — GM "Enter Table →" anchor "dead" on the WebKitGTK desktop shell

### Root cause (with console evidence — captured live via the WebKitGTK 2.50.4 remote inspector)

The anchor was **never dead**. Instrumented pointer/click capture inside the real
WebKitGTK webview (raw `WEBKIT_INSPECTOR_SERVER` GVariant client; see
"Inspector channel" below) shows the full event path working:

```
[22560] click @anchor bubble target=A defaultPrevented=false
[22560] click @document bubble target=A defaultPrevented=true   ← SvelteKit router consumed it
[22560] click @window bubble target=A defaultPrevented=true
[22578] history.pushState -> http://localhost:1420/table/play
[22579] window.onerror: Svelte error: bind_invalid_checkbox_value
        Using `bind:value` together with a checkbox input is not allowed.
        Use `bind:checked` instead …in +page.svelte  in +layout.svelte
```

The router pushState fires and the URL **does** change to `/table/play` — then the
play page's render aborts mid-mount on a thrown Svelte runtime error, the component
tree never appears, and the URL change is invisible. Same crash explains the
"silent `goto('/table/play')`" during hosting (evlog `[16903] history.pushState ->
…/table/play` + `[16907] window.onerror: Svelte error: bind_invalid_checkbox_value`)
and why the evening device-lab desktop shots all show the lobby.

The thrown rule violation, verbatim from the Console domain (buffered replay at
`Console.enable`, plus a second console.error at click time):

```
[console.error] Svelte error: bind_invalid_checkbox_value
Using `bind:value` together with a checkbox input is not allowed.
Use `bind:checked` instead
    in <unknown>  in +page.svelte  in +layout.svelte  in root.svelte
```

Offending line: `src/routes/table/play/+page.svelte` —
`<input type="checkbox" bind:value={diceSecret} />` for the quick-roll "Secret"
toggle (`let diceSecret = $state(false)` → boolean).

**Why no existing gate caught it:**
- `bind_invalid_checkbox_value` is a **dev-only** Svelte throw — the harness e2e
  (`vite build` prod bundle) compiles it out; prod silently mis-binds instead.
- `svelte-check` (0/0 before and after) does not flag it as a diagnostic.
- The evening desktop app ran `tauri dev` (vite dev server) → DEV Svelte → the
  throw aborted every `/table/play` mount. Release (prod) bundles were unaffected
  but the toggle itself was still misbound there; the fix is correct for both.

### Fix
One-line binding correction on the play page:
`bind:value={diceSecret}` → `bind:checked={diceSecret}`, with an F1-style
diagnosis comment at the site. Semantics are what the code always intended —
`rollDice(diceInput, diceSecret)` reads the same boolean `$state`.

### Post-fix verification (same inspector channel, fresh app)
- Host flow: `{ path: "/table/play", play: true, errs: [] }` — `goto("/table/play")`
  lands, `.play-surface` mounts, zero page errors/exceptions.
- Lobby → anchor click on "Enter Table →" is the identical code path the
  instrumented click already traced through router interception + pushState;
  with the render crash gone there is no remaining failure point.
- The invite panel only exists while a session is still connected; leaving
  `/table/play` tears the session down (pre-existing transport lifecycle — the
  evening's "stuck on lobby WITH invite panel" state was itself a product of the
  crash). Out of scope here.

## F-R6 — Native lobby "Join Session" button under the fixed tabbar

### Diagnosis (measured, harness build at real viewports)
- Phone-class 390×867 (LAN-prefilled lobby, matching `24-join-error-native.png`
  intent): document 1100px tall; Join submit's natural box doc y≈963–1003 vs
  tabbar top edge 814.83 (tabbar measures 52.17px, `h` invariant across
  390/800/1280 widths) → entirely below the viewport at rest, and inside the
  tabbar's 52px occlusion band during partial scroll.
- `main { padding: 1rem 1rem 5rem }` was considered and **rejected with measured
  proof**: padding only buys scroll range — the max-scroll clearance vs the
  tabbar is a constant ≈27.8px (= F1's 27.6px) independent of padding, and
  rest-state doc coordinates are padding-independent. A padding bump cannot
  un-occlude the button.
- `position: sticky; bottom: …` on the button was the next candidate and is
  **also not viable, empirically**: sticky travel is clamped to the containing
  block — the button is the `<form>`'s last in-flow child, so its travel room
  is exactly its own 8px top margin (measured: form content top 724.9 vs button
  top 732.9 at 800×600); it can never float 200px up to the clearance line.

### Fix
`<form class="join-form" …>` (new class hook) + a scoped
`@media (max-width: 520px)` rule making the Join submit a **fixed CTA** pinned
`calc(52px + 0.75rem)` above the viewport bottom with an opaque background
(tabbar recipe, primary tint, light + dark) — phone widths only; ≥520px is
untouched (the 800×600 desktop window measured full clearance at scroll).
Diagnosis comment with all four measurements lives at the rule.

### Post-fix geometry (probe, harness build)
- 390×844 (S27 viewport): Join `[739.8, 780.0]` vs tabbar top `791.83` — clear by
  11.8px at rest **and** at max scroll (position-scroll independent).
- 390×867: Join `[762.8, 803.0]` vs tabbar top `814.83` — clear by 11.8px.
- 800×600 (evening desktop window): media query inert; natural flow vertically
  identical to pre-fix; clears by 45px at max scroll as before.
- Horizontal overflow unchanged (`scrollW = 390` everywhere).

### Regression assertion
`tests/e2e/mobile.spec.ts` S27 stop 2 now asserts
`joinButton.bottom ≤ tabbar.top + 1` at scroll 0 — the exact mirror of stop 3's
F1 composer check. Passes post-fix.

## Inspector channel (how the console evidence was captured)

`WEBKIT_INSPECTOR_SERVER=127.0.0.1:12323` under Xvfb does not speak HTTP/JSON
(curl gets an empty reply): it is a raw-TCP GLib `SocketConnection` speaking
GVariant-framed messages. A minimal Python client (framing
`uint32-be size | u8 flags | name\0 | GVariant body`) connects with
`SetupInspectorClient(ay)` → `DidSetupInspectorClient` + `SetTargetList`, pairs
with `Setup(tt)`. **The second trap:** at that level every inspector domain
returns `{"code":-32601,"message":"'Runtime' domain was not found"}` — pairing
lands on the UI-process `WebPageInspectorController` shell, which registers
only `Target` + `Browser` agents. The real page domains are reached via the
multi-target protocol: read `Target.targetCreated` (`targetId: "page-N"`), then
wrap every call as `Target.sendMessageToTarget { targetId, message }`,
unwrapping `Target.dispatchMessageFromTarget` events. `Console.enable` +
`Runtime.enable` + `Runtime.evaluate` all work there; `Log.enable` legitimately
does not exist in WebKit. One discipline note: the inspector server wedges the
target side after a full client session disconnects (empty target lists on
reconnect) — relaunch the app per debugging session.

## Gates

- `corepack pnpm check` — 0 errors / 0 warnings
- `corepack pnpm test` — 131/131
- `corepack pnpm e2e` — 27/27 (incl. strengthened S27)
- `PF2E_E2E_LAN=1 corepack pnpm e2e:live` — 4/4 (justified: the F-R3 fix changes
  play-page runtime binding beyond CSS; L1 flaked once on first full-lane run,
  then passed in isolation (913ms) and on the complete rerun — recorded as a
  flake, not a regression)
- `corepack pnpm build` — clean
- `corepack pnpm e2e:shots` — 80 PNGs, **zero page/console errors**
- No `src-tauri` source changes (only the gitignored `target/` example build for
  the live lane) — cargo battery not applicable.

## Checklist status

- [x] Bottom-tab reachability on mobile: Join CTA pinned fully above the tabbar
  at rest at 390px; S27 (5 stops) green including the new stop-2 assertion.
- [x] No horizontal overflow: S27 strict `scrollWidth ≤ 390` green post-fix.
- [x] Desktop layout unchanged: F-R6 rule is `@media (max-width: 520px)`-scoped;
  F-R3 touches a binding, not layout; desktop desktop shots byte-identical
  compositions.
- [x] No tabbar/design drift: tabbar position/height/labels untouched; the F-R6
  CTA reuses the tabbar opaque-background idiom per theme.
- [x] Native evidence attached: full console capture + event timeline for F-R3,
  real-viewport geometry numbers pre/post for F-R6.
