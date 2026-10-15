# PLAN — Tier 1 slice: UI screenshot harness for agent-consumable visual feedback

Status: PLANNED (not implemented, rev 2). Goal: **one command → deterministic
PNG screenshots of every meaningful UI surface (desktop + mobile, light +
dark) → agents review the PNGs with the Read tool and file UI feedback under
`docs/ui-reviews/`.** Frontend-only; zero Rust changes; no changes to any IPC
command, wire format, or existing test.

Binding maintainer decisions (rev 2): **Chromium-only**; **full matrix** (no
dark-mode trimming); **include the scripted chat-reply variant in v1**
(tokens replayed through the real `llm:token` listen path, stubbed backend,
labeled as such); **harness code must be type-checked by `pnpm check`**
(tsconfig include extended to `e2e/`, §2 Step 5).

Rev 2 also absorbs the landed first-run wizard (uncommitted tree): new route
`/onboarding` (3 stepper screens in one page), new IPC `wizard_status` /
`wizard_validate_vault` / `wizard_complete` / `app_restart` (bindings at
`src/lib/ipc.ts:500-523`), new `src-tauri/src/settings.rs`, lens persistence
via localStorage `pf2e.lens` (`src/lib/lens.svelte.ts:4`), and a
`+layout.svelte` onMount wizard redirect (`src/routes/+layout.svelte:21-33`)
that the fixture layer MUST neutralize (§5 hazard).

Out of scope (respected): heavyweight visual regression diffing (no
pixel-comparison gate); anything running in default `pnpm test`.

## 1. The verified Tauri-IPC-in-a-browser contract

The whole harness rests on one fact, verified against
`node_modules/@tauri-apps/api` **v2.11.0** (the installed version, not the web).
`core.js:201-203` — every `invoke()` from `@tauri-apps/api/core` (which
`src/lib/ipc.ts:2` re-exports for **all app commands**) delegates to a single
browser global:

```js
async function invoke(cmd, args = {}, options) {
    return window.__TAURI_INTERNALS__.invoke(cmd, args, options);
}
```

`core.js:69-73` — `transformCallback` (needed by `Channel` and by `listen`)
is also just a global:

```js
function transformCallback(callback, once = false) {
    return window.__TAURI_INTERNALS__.transformCallback(callback, once);
}
```

`event.js:71-83` — `listen()` (used by `src/lib/transport.ts:40-47` for
`session:patch`/`session:presence`, and `src/lib/llm.svelte.ts:41` for
`llm:token`) is itself only an `invoke` + `transformCallback`:

```js
return invoke('plugin:event|listen', {
    event,
    target,
    handler: transformCallback(handler)
}).then((eventId) => { return async () => _unlisten(event, eventId); });
```

`event.js:42-47` — unlistening touches one *second* global, synchronously:

```js
window.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener(event, eventId);
await invoke('plugin:event|unlisten', { event, eventId });
```

`plugin-dialog` (`node_modules/@tauri-apps/plugin-dialog/dist-js/index.js:124`,
used by `src/lib/savedSessions.svelte.ts:39-42`) is likewise plain IPC —
`return await invoke('plugin:dialog|save', { options });` — and
`transport.ts:324-327` picks the IPC transport **iff the global exists**:

```ts
// @ts-expect-error Tauri injects this global in desktop builds
if (typeof window !== "undefined" && window.__TAURI_INTERNALS__) {
    return ipcTransport();
```

**Therefore the minimal stub surface is exactly** (mirroring the official
mock in `@tauri-apps/api/mocks.js:160-166`, which installs precisely these):
`window.__TAURI_INTERNALS__ = { invoke, transformCallback, unregisterCallback }`
plus `window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener }`.
Verified NOT needed: `metadata` (no `@tauri-apps/api/window`/`webview`
imports in `src/`), `convertFileSrc` (unused), `Channel` (LLM streaming uses
`listen`, not Channels), `mockWindows`, and the `plugin-opener` package
(in `package.json` deps but zero imports in `src/`).

Because Playwright `context.addInitScript` runs **before any page script**
(and the SvelteKit bundle is deferred module JS), installing these globals
boots the app in a plain browser with zero app-code changes. The stub replays
events via a `window.__e2eEmit(event, payload)` hook (the mock's
`runCallback` pattern) — this is how `/table/play` gets a snapshot without a
live host.

## 2. Implementation steps

Paths relative to `pf2e-companion/` (project root; git root one level up).
Node is `~/.nvm/versions/node/v24.12.0/bin` (type-stripping default-on, so
`node e2e/*.ts` runs directly — no tsx dep).

### Step 1 — devDependency: `@playwright/test` (~1 line)

`pnpm add -D @playwright/test` (pin style `^1.x` matching every other
devDep's caret style; it transitively provides `playwright`, and re-exports
`chromium`). Chromium only — the app ships in a WebKit/WebView2 Tauri shell,
but the point is layout/state review, not engine parity (caveat in §5).

Browser binaries: this sandbox already has `~/.cache/ms-playwright/
chromium-1243` + `chromium_headless_shell-1243` cached (verified). Fresh
machines/CI run `pnpm exec playwright install chromium` (~170 MB download);
document `PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1` for CI installs that never run
shots. Vitest conflict: none — `vite.config.js` `test.include` is
`src/**/*.test.ts`, so `e2e/` is invisible to `pnpm test` by construction.

Effort: ~10 min.

### Step 2 — `e2e/tauri-stub.js` (new, ~90 lines, plain JS)

Passed to `context.addInitScript({ path: "e2e/tauri-stub.js" })`. Self-contained
(no imports — init scripts are injected, not bundled). Contents:

- Callback registry `Map<id, fn>` with deterministic ids (start at 1);
  `transformCallback(cb)` registers, `unregisterCallback` deletes,
  `runCallback(id, payload)` invokes.
- Event registry `Map<event, Set<id>>`; `invoke` special-cases
  `plugin:event|listen` (register, return the handler id) and
  `plugin:event|unlisten` (return null). Exposes
  `window.__e2eEmit(event, payload)` → calls every registered handler with
  `{ event, id, payload }` (the shape `listen` callbacks expect — same as
  `mocks.js` `handleEmit`).
- `window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener }`.
- **Scripted LLM replay** (drives the v1 chat-reply variant): special-cases
  `llm_chat` — mints a fixed session id (`"e2e-chat-1"`), stores a pending
  replay `{session_id, events[]}` (6 `token` fragments, 1
  `tool_start`/`tool_result` pair to exercise the trace UI, 1 `done` with
  fixed usage numbers), and **returns the session id immediately** — the real
  contract: `chat/+page.svelte:91-99` awaits `llmChat(...)` for the session
  id, then `subscribeToSession(sessionId, …)` registers the `llm:token`
  listener. The replay flushes **on first `plugin:event|listen` for
  `"llm:token"` when a replay is pending** (deterministic hand-off, no
  race), one event per 30 ms `setTimeout`.
- `wizard_validate_vault` special-case: echo `args.path` minus trailing `/`
  (the "normalized path" the wizard stores) — arg-sensitive, so it lives in
  the stub, not the JSON fixture map.
- Fixture dispatch: reads `window.__E2E_FIXTURES__` (plain JSON installed by a
  *preceding* init script carrying the chosen **variant** map from Node).
  `invoke(cmd, args)` returns a `structuredClone()` of the fixture (per-call
  clone so one page's mutations can't leak into the next shot); unknown `cmd`
  **throws** `e2e: unknown command <cmd>` — loud failure, never a silent
  blank page. Arg-sensitive commands (`search`, `get_entity`,
  `list_entities`) use `{ __by: "id" }` maps the stub resolves.

Effort: ~1.5 h including manual boot verification.

### Step 3 — `e2e/fixtures.ts` (new, ~280 lines)

Typed fixture map keyed by command name, shapes imported from `$lib/ipc`
(`import type { ... } from "../src/lib/ipc"`) so drift from the real bindings
is a type error. Exported as **`base` + named variants** (`default`,
`wizardFresh`, `chatConfigured`) — the runner picks one per matrix row and
injects only that map. `default` is the union below with
`wizard_status.needed === false`; `wizardFresh` = `{ ...base, wizard_status:
{ needed: true, … } }`; `chatConfigured` = `{ ...base, llm_status:
{ configured: true, provider: "ollama", model: "qwen3:4b", key_present:
false } }` — the chat page gates sending on `llmState.status?.configured`
(`chat/+page.svelte:71-74`).

| Command | Fixture (all timestamps fixed epoch `T0 = 1_760_000_000_000`) |
|---|---|
| `search` | 4 `SearchHit` ("Hosts" query default on `/`): mixed types, `source` `"both"/"fts"/"vec"`, stable scores |
| `schema_version` | `3` |
| `list_lenses` | the exact 5 packs from `commands.rs:910-935` (lewisian/catholic/reformed/pentecostal/…, copy labels verbatim) |
| `list_campaigns` | 2: `Campaign { id: "c-runes", name: "Rise of the Runelords", default_lens: "lewisian", entity_count: 12 }`, `"c-homebrew"` |
| `lookup_alias` / `lookup_miracle` | 2 `AliasHit` (Magic Missile → Force Barrage …) / 2 `MiracleHit` (Mt 14:25 …) |
| `xp_budget` / `creature_xp` | `XpBudgetResult` party 4 / moderate; `20` for delta −1 |
| `list_entities` | 8 `EntitySummary` across creature/npc/deity/location/item/spell, mixed lenses, some `lens: null` |
| `get_entity` | `EntityDetail` map by id: one creature **with** statblock + body markdown, one deity with frontmatter only — covers `/entity/[id]`, `/vault/[id]`, `Sources`/`Statblock` components |
| `list_relations` | 2 `RelationRow` for the creature entity |
| `llm_status` | `{ configured: false, provider: null, model: null, key_present: false }` (honest: the default friends-beta state) |
| `rag_index_stats` | `{ indexed: true, entities: 12, chunks: 41, provider: "ollama", model: "nomic-embed-text" }` |
| `eval_load_suite` | 2 `EvalPrompt` with mixed `Expectation` kinds |
| `session_list_saved` | 2 `SavedSessionSummary` (one ended, one live; `saved_at`/`created_at` = T0) |
| `session_host` / `session_load` | `SessionInfo { role: "gm", session_id: "e2e-session", peer_id: "e2e-peer-gm-0001", lan_url: "http://192.168.1.42:4317/e2e-session", p2p_ticket: "e2e-ticket", name: "GM", pin: "1234" }` |
| `session_intent` / `session_leave` | `null` |
| `diagnostics_get` | `DiagnosticsInfo` with 6 fixed log lines (no secrets — mirrors the Phase 13 guardrail) |
| `plugin:dialog|save` / `plugin:dialog|open` | save → `"/home/player/Downloads/aboleth-tide-2025-10-09.log.md"` (export click only); open → `null` (cancelled picker — onboarding's "Choose folder…" degrades gracefully) |
| **`wizard_status`** | **DEFAULT — CRITICAL**: `WizardStatus { needed: false, default_vault: "/home/player/.local/share/pf2e-companion/vault" }` (`ipc.ts:502-512`). `needed: true` in the DEFAULT variant redirects every shot to `/onboarding` (§5 hazard). `wizardFresh` flips to `needed: true` |
| **`wizard_validate_vault`** | stub-side echo of `args.path` (normalized) — Step 2 |
| **`wizard_complete` / `app_restart`** | `{ restart_required: false }` (Finish lands on `goto("/")`, not the restart card); `app_restart` → `null` (unreachable; exploration safety) |
| `create_campaign`/`create_entity`/`update_entity`/`delete_entity`/`add_relation`/`delete_relation`/`llm_configure`/`llm_clear_config`/`llm_chat` (replay — Step 2)/`rag_reindex`/`eval_run`/`session_join_peer`/`session_join_lan`/`session_export_log`/`import_foundry_pack`/`validate_statblock` | minimal typed stubs (not exercised by the default matrix; present so interactive exploration in `--debug` never hits the loud unknown-command throw) |

Also export `SNAPSHOT` — a full `SessionSnapshot` (GM + 2 players with
character sheets, 6 chat entries incl. one roll with `kept`, initiative
order with active idx, 12×8 scene grid with 3 tokens, 1 revealed + 1 GM-only
handout) for `/table/play` via `__e2eEmit("session:patch", { session_id:
"e2e-session", kind: "snapshot", snapshot })` — accepted by `applyPatch`
(`session.svelte.ts:16-40` drops patches only when `info` is set AND ids
mismatch).

Effort: ~2.5 h.

### Step 4 — `e2e/matrix.ts` (new, ~120 lines)

The route × viewport × theme table + per-route setup. Viewports:
`desktop = 1280×800` (dsf 1), `mobile = 390×844` (dsf 2, `isMobile`,
`hasTouch` — exercises the bottom tabbar layout). Themes: `light` + `dark`
via context `colorScheme` (theming verified: `+layout.svelte` sets
`color-scheme: light dark` + a `prefers-color-scheme: dark` tabbar rule).

| # | Route | Setup beyond `goto` | Ready selector |
|---|---|---|---|
| 1 | `/` | — | `.hit` (first search result) |
| 2 | `/encounter` | — | XP budget result `.num` or panel root |
| 3 | `/vault` | — | entity list item |
| 4 | `/vault/new` | — | `EntityEditor` form fields |
| 5 | `/vault/e2e-aboleth` | fixture id | editor populated (`input` value) |
| 6 | `/entity/e2e-aboleth` | fixture id | `.statblock` or markdown body |
| 7 | `/aliases` | — | alias result row |
| 8 | `/miracles` | — | miracle result row |
| 9 | `/table` (lobby) | — | saved-session list row |
| 10 | `/table/play` **gm** | scripted lobby flow: fill name "GM", select campaign, click Host (`session_host` fixture) → auto-`goto("/table/play")`; then `__e2eEmit` snapshot | `.play-tabs` + a chat entry |
| 11 | `/table/play` **player** | direct `goto("/table/play?name=Rook")`, transport connects via stub, then `__e2eEmit` snapshot | same, no GM tab |
| 12 | `/chat` | — | unconfigured-LLM notice |
| 13 | `/settings` | — | diagnostics card |
| 14 | `/settings/llm` | — | RAG stats card |
| 15 | `/settings/eval` | — | suite table row |
| 16 | `/about` | — | content section |
| 17 | `/onboarding` screen 1 **Vault** | `wizardFresh` variant; direct `goto` (redirect is suppressed inside `/onboarding` by `shouldRedirectToWizard`, `wizard.svelte.ts:16-18`) | header text `Step 1 of 3 · Vault` |
| 18 | `/onboarding` screen 2 **Lens** | continue from 17: click the **Next** button (real UI path — `wizardState.step` rune via `next()`, `wizard.svelte.ts:28-30`) | `Step 2 of 3 · Lens` |
| 19 | `/onboarding` screen 3 **LLM** | click Next again; optionally click the `anthropic` radio to also show the API-key field before shooting | `Step 3 of 3 · LLM` |
| 20 | `/chat` **scripted reply** | `chatConfigured` variant; fill draft, click send → `llm_chat` stub returns `e2e-chat-1` → replay flushes on the page's real `llm:token` `listen` (Step 2); wait for the `done` event (assistant text stable) | assistant message with full reply + tool trace |

20 rows × 2 viewports × 2 themes = **80 PNGs**. `main` padding-bottom 5rem vs
the fixed tabbar is exactly the class of bug mobile shots exist to catch.

Wizard driving notes: the 3 screens are ONE route driven by `wizardState.step`
(`src/lib/wizard.svelte.ts:5-12`) — advanced by real clicks on the Next
button (module runes aren't reachable from page context, and clicks are the
honest path). Verify Next is enabled with the default-vault choice preselected
(`useDefaultVault: true` initial state); if gated, click "Use default" first.
Row 17's fresh context keeps `localStorage` empty → lens starts at
`lewisian` (`pf2e.lens`, `lens.svelte.ts:4-7`). A 4th onboarding row (the
"Restarting…" card via a `restart_required: true` micro-variant) is an
optional stretch — not counted in the 80.

Effort: ~2.5 h (wizard flow + chat replay).

### Step 5 — `e2e/shots.ts` (new, ~180 lines) — the runner

1. `pnpm build` (skip with `SHOTS_SKIP_BUILD=1` if `build/` is fresh).
2. Spawn `pnpm preview --port 4173 --strictPort` (child process; kill on
   exit). **Preview of the static build, not dev server**: no HMR websocket
   (so `networkidle` settles), no dev overlay, prod CSS. SPA deep links work —
   verified in `@sveltejs/kit`'s preview middleware (non-prerendered paths
   fall through to the server handler, which with `ssr = false` + adapter
   `fallback: index.html` returns the SPA shell). Gate step 0: `curl -s
   localhost:4173/table/play` returns shell HTML; if it 404s, fallback =
   `goto("/")` + client-side navigation via tabbar links.
3. `chromium.launch({ headless: true, args: ["--hide-scrollbars", "--force-color-profile=srgb", "--font-render-hinting=none"] })`.
4. **Startup assertion (hard fail before any browser work)**:
   `fixtures.variants.default["wizard_status"].needed === false` — guards
   the §5 redirect hazard at the earliest possible moment.
5. Per matrix row: fresh context (init scripts: fixture-injector with the
   row's **variant** map, then `tauri-stub.js`), run setup, wait for ready
   selector + `networkidle` + 300 ms settle, then
   `page.screenshot({ fullPage: true })`.
6. Write `shots/<route-slug>/<viewport>-<theme>.png` and accumulate
   `shots/manifest.json`: `{ generatedBy, node, playwright, routes: [{ route,
   variant?, viewport, theme, file, setup, readySelector, notes }] }` — an
   agent enumerates surfaces from the manifest, no globbing. Chat-reply rows
   carry `notes: "stubbed-backend: llm:token replay"`; play rows carry
   `notes: "fixture-snapshot"`.
7. Collect `pageerror` + `console.error` per context → print a per-route
   summary; **any error fails the run (exit 1)**; otherwise exit 0.

`package.json`: add `"e2e:shots": "node e2e/shots.ts"`. `.gitignore`: add
`shots/`. **Type-check wiring (binding decision)**: `.svelte-kit/tsconfig.json`
includes only `src/`, `test/`, `tests/` — the root `tsconfig.json` gets an
`include` array that **copies the full generated list verbatim** (TS does not
merge includes; a partial copy silently unchecks `src/`) and appends
`"../e2e/**/*.ts"` + `"../e2e/**/*.js"` — the plain-JS stub is checked too via
the existing `allowJs`/`checkJs` (lines 4-5). Gate: `pnpm check` stays
0 errors / 0 warnings with the harness included.

Effort: ~2 h.

### Step 6 — `docs/ui-reviews/README.md` (new, ~90 lines) — the feedback loop

Protocol doc: (a) run `pnpm e2e:shots`; (b) read `shots/manifest.json`;
(c) Read each PNG (Read tool renders images); (d) check the invariant
checklist; (e) write findings to `docs/ui-reviews/<YYYY-MM-DD>-<route>.md`
(template: route/viewport/theme, severity, screenshot file, suggested fix).
Checklist: text overflow/clipping; empty states; all 6 tabbar tabs +
About/Chat/Settings pills reachable at 390px (no horizontal scroll, nothing
under the fixed tabbar); dark-mode contrast (`color-mix` tokens); broken
images/emoji icons; form alignment; GM-only elements hidden for the player
variant; wizard invariants ("Step N of 3 · …" matches the fieldset, radios
focusable, Back/Next reachable on mobile); chat-reply variant (tool traces,
no orphaned empty assistant bubble). Plus a **copy-paste reviewer-subagent
prompt template**: "You are a UI reviewer for pf2e-companion. Run
`pnpm e2e:shots`, read `shots/manifest.json`, Read every PNG listed, evaluate
each against the checklist in docs/ui-reviews/README.md, then write one
finding file per route with defects ranked by severity. Do not modify app
code." The doc states honestly what the shots do NOT prove (see §4).

Effort: ~1 h.

### Step 7 — README.md (~6 lines)

Under Develop: `pnpm e2e:shots` — what it does, the one-time
`playwright install chromium` requirement, `shots/` is gitignored, pointer to
`docs/ui-reviews/README.md`.

Effort: ~10 min.

## 3. Test plan / gates

- **No new vitest files, no cargo tests, no Rust changes.** Existing gates
  stay green untouched: `pnpm test` (~100 tests; e2e/ outside
  `test.include`), `pnpm check` (0/0 — now also type-checking e2e/),
  `pnpm build`, `cd src-tauri && PKG_CONFIG_PATH=/tmp/opencode/gtk-shim/pkgconfig
  cargo test`.
- **The slice's own gate**: `pnpm e2e:shots` exits 0 headlessly in this
  sandbox (browsers cached at `~/.cache/ms-playwright`; otherwise
  `pnpm exec playwright install chromium` first), producing 80 PNGs +
  `shots/manifest.json`, zero page/console errors, passing the
  `wizard_status.needed === false` startup assertion.
- **Determinism spot-gate** (manual, not committed): run twice, `sha256sum`
  all PNGs → identical.
- **Opt-in posture**: `e2e:shots` is never invoked by `pnpm test`,
  `pnpm check`, or CI defaults — lighter even than `PF2E_GUI_SMOKE` (no
  Tauri, no xvfb, no network beyond localhost).

## 4. What honestly CANNOT be screenshotted (and the honest stub or skip)

- **Real LLM behavior**: the chat-reply variant replays fixed tokens through
  the **real** `llmChat`-returns-session-id → `subscribeToSession` →
  `listen` → `llm:token` path, so streaming UI, tool traces, and `done`
  rendering are genuine components — but the *backend* is canned (labeled
  `stubbed-backend`). Token cadence and provider errors: not represented.
  Default `/chat` shows the true unconfigured state.
- **Real automerge/p2p sync** (second client, `peer_joined` presence,
  reconnect banners, LAN QR/ticket flow): needs a real host. Play-surface
  shots render the real UI against a fixture snapshot pushed through the
  real `listen`→`applyPatch` path — labeled `fixture-snapshot`.
- **Wizard restart card + vault-move semantics**: `wizard_complete` stubbed
  to `restart_required: false`; nothing restarts, no vault files touched.
  The 3 stepper screens are the real UI; persistence outcomes unexercised.
- **Native OS dialogs** (`plugin:dialog`): stubbed (path / `null`).
- **Tauri window chrome, pinch-zoom, touch token-drag**: static PNGs can't
  prove gestures; manual checklist items in the review README.
- **Real WebKit/WebView2 rendering**: Chromium shots; cross-engine drift
  accepted (§5). **Foundry import UI**: file-picker driven, not in matrix.

## 5. Risks / unknowns

- **⚠ HAZARD — the `wizard_status` redirect loop (highest-severity risk)**:
  `+layout.svelte:21-33` calls `wizardStatus()` on every app mount and
  redirects to `/onboarding` when `shouldRedirectToWizard(pathname, needed)`
  is true — whenever the fixture says `needed: true` and the page isn't
  already `/onboarding` (`wizard.svelte.ts:16-18`). If the DEFAULT variant
  ever returns `needed: true` (copy-paste from `wizardFresh`, bad merge, or
  pasting the wizard's real first-run response to "fix" a missing command),
  **every screenshot silently becomes the wizard's first screen** — 80
  near-identical PNGs, findings garbage, possibly a green exit (layout chrome
  like `.tabbar` still renders on `/onboarding`). Three required mitigations:
  (a) `wizardFresh` is a spread **override on `default`**, never standalone;
  (b) the runner's startup assertion (Step 5.4) hard-fails unless
  `variants.default["wizard_status"].needed === false`; (c) ready selectors
  are route-*specific* content (`Step 1 of 3 · Vault`, hit rows,
  statblocks), not layout chrome. If `wizard_status` is merely ABSENT:
  `invoke` throws → layout catches (`+layout.svelte:30`) → no redirect —
  safe, and the loud unknown-command console error trips exit 1 anyway.
- **Replay-vs-subscribe race (chat variant)**: the page subscribes to
  `llm:token` only AFTER `llmChat` resolves (`chat/+page.svelte:91-99`), so
  naive "emit on invoke" loses every token. Flush-on-first-listen (Step 2)
  is the deterministic hand-off; robust even if order changes later.
- **SPA fallback in `vite preview`**: verified in Kit's preview middleware
  source; verify empirically at Step 5 start (curl deep route). Fallback:
  client-side navigation from `/` — supported by the design.
- **`node e2e/shots.ts` type stripping**: Node 24 strips types by default;
  erasable syntax only (no enums/namespaces). `pnpm check` verifies the types.
- **localStorage `pf2e.lens`**: read at module init (`lens.svelte.ts:6-7`),
  written by a layout `$effect` — a shared context would leak lens choice
  between routes. Fresh context per shot makes this moot; do NOT "optimize"
  into one long-lived page.
- **Font/emoji nondeterminism across machines**: no Inter webfont loaded;
  emoji icons come from host fonts. Deterministic per-machine, not
  byte-identical across OSes — acceptable.
- **Init-script ordering** (fixture-injector first, stub second — both before
  any module script) and **`structuredClone` per invoke** (no cross-shot
  mutation leaks via Svelte 5 `$state` proxies): both are load-bearing
  details; a refactor violating either trips the loud unknown-command throw
  or produces drifted fixtures.
- **Playwright version ↔ cached browser drift**: upgrades demand
  `playwright install chromium` again; the runner prints a hint when launch
  fails with "Executable doesn't exist".
- **svelte-check on e2e/**: the tsconfig include edit must copy the whole
  generated list (a partial copy silently unchecks `src/`). The wizard tree
  is uncommitted and under review: if `wizard_*` bindings change shape,
  `fixtures.ts` becomes a type error (the point of importing the types) —
  re-read `ipc.ts:500-523` at implementation time.
- **80 PNGs × fullPage**: clip any PNG over ~2 MB (manifest lists file sizes
  for the agent's Read budget).
- **Lockfile**: `pnpm-lock.yaml` changes (one new devDep) — the only
  lockfile delta; `Cargo.lock` untouched.

## 6. Acceptance criteria

1. `pnpm e2e:shots` (one-time `playwright install chromium` on fresh
   machines; already cached here) builds, serves the static build on :4173,
   and exits 0 headlessly with **zero page/console errors**; two consecutive
   runs are byte-identical (spot-gate).
2. `shots/` contains 80 PNGs matching the §2 matrix (20 rows × 2 viewports ×
   2 themes) plus `manifest.json` fully enumerating them (route, variant,
   viewport, theme, file, notes); `shots/` is gitignored.
3. `/table/play` renders GM and player variants from the fixture snapshot
   through the real `listen` → `applyPatch` path; GM-only tabs visible only
   in the GM variant.
4. `/onboarding` renders all 3 stepper screens (Vault / Lens / LLM),
   advanced by real Next-button clicks, in the `wizardFresh` variant — and
   NO other route redirects to `/onboarding` (`needed: false` + the startup
   assertion prove it).
5. The `/chat` scripted-reply variant shows an assistant message with tool
   traces assembled from `llm:token` events delivered through the page's real
   `subscribeToSession` listener; labeled `stubbed-backend`.
6. An agent following `docs/ui-reviews/README.md` goes from zero to a
   written per-route findings file without globbing or app knowledge.
7. All pre-existing gates stay green: `pnpm test`, `pnpm check` (0/0 —
   covering `e2e/**` per the binding decision), `pnpm build`, `cargo test`
   (default + `--all-features`). Only `pnpm-lock.yaml` changes among
   lockfiles.
