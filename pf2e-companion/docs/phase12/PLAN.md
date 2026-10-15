# Phase 12 — UI + Gameflow E2E Harness (PLAN)

Status: **landed** (2026-10-14 — 12.0–12.3 all landed, gates in §9). This
document is the contract for the Phase 12 end-to-end
test harness. It extends the frozen contracts in `docs/phase10/` (CONTRACT.md,
CONTRACT-10_5.md, CONTRACT-11.md) and changes none of them. Guardrails in
`docs/SHIPPING.md` ("Standing guardrails") apply to everything here.

All paths below are relative to the app directory `pf2e-companion/` (the git
repo root is the parent). The two pre-existing test surfaces stay untouched:
`vitest` (`src/**/*.test.ts`, env `node`) and `cargo test` (`src-tauri/tests/`).
The env-gated boot check `src-tauri/tests/gui_smoke.sh` (`PF2E_GUI_SMOKE=1`)
continues to be the only real-binary GUI gate.

## 1. Scope + goals + non-goals

### Goal

Drive every UI route and the full two-client table gameflow in a **real browser
engine**, deterministically in CI, capturing screenshots as reviewable evidence
of each gameflow step. The harness proves: (a) every route renders its states
(data, empty, error) correctly; (b) every UI control emits the **exact IPC
intent shape** the Rust authority layer expects (intent wiring); (c) the play
surface renders every `PatchEvent`/`PresenceEvent` kind correctly (render
wiring); (d) the GM-view vs player-view divergence (secret rolls, hidden
handouts, GM-only tabs) holds in the DOM.

### In scope

- Chromium-based E2E (“the deterministic lane”), default-on in CI, no display
  server, no WAN, no Tauri shell.
- Two-browser-context GM+player gameflow with a scripted patch stream (the
  test script plays the server).
- Screenshot capture per scenario step, uploaded as CI artifacts.
- An env-gated loopback lane against the real Rust `lan` WebSocket server
  (`PF2E_E2E_LAN=1`, mirrors the `PF2E_NET_TESTS=1` / `PF2E_GUI_SMOKE=1`
  pattern), covering the real `wsTransport` receive path.
- Mobile viewport (390×844) passes for bottom-tabs layout and the play surface.

### Non-goals (v1)

- **Visual regression baselines.** Screenshots are evidence-only (§5). No
  `toHaveScreenshot()` baselines; cross-runner font AA makes them brittle on
  day one. Revisit trigger is documented in §5.
- **tauri-driver / WebKitGTK input automation.** Evaluated and rejected as the
  v1 tier-1 (§2). `gui_smoke.sh` remains the real-binary gate.
- **Real iroh P2P in the browser.** Cargo already owns P2P convergence
  (`tests/phase10_p2p.rs`, env-gated). Browser-side iroh is impossible anyway
  (WS bridge is the mobile path by design).
- **LLM token streaming** in the harness. `/chat` and `/settings/*` are
  covered in their LLM-off / fixture-run states only (§4, group C). The mock
  event bus (`window.__pf2eHarness.emit("llm:token", …)`) supports a future
  streaming scenario without architecture changes.
- **Automerge fidelity.** The mocked lane never touches automerge; the live
  lane asserts UI-visible outcomes only, not doc internals.
- Firefox/WebKit browser projects (Chromium only, §2.3).

## 2. Tooling decision

### 2.1 Recommendation (tier 1)

**Playwright (`@playwright/test`) driving a Vite `harness`-mode build of the
app in headless Chromium, with `@tauri-apps/api/*` aliased to in-repo mocks.**

Rationale, grounded in this repo:

1. **The app is a pure SPA with a total, enumerable Tauri surface.**
   `adapter-static` + `ssr = false` (`svelte.config.js`, `src/routes/+layout.ts`).
   Every Tauri touchpoint is one of exactly two module imports — the full set:
   - `src/lib/ipc.ts:2` — `import { invoke } from "@tauri-apps/api/core"`
   - `src/lib/llm.svelte.ts:2` — `import { listen } from "@tauri-apps/api/event"`
   - `src/lib/transport.ts:40` — dynamic `import("@tauri-apps/api/event")`
   - `src/routes/table/+page.svelte:87` — dynamic `import("@tauri-apps/api/core")`

   A Vite `resolve.alias` of those two specifiers covers 100% of the app's
   platform bridge. `@tauri-apps/plugin-opener` appears in `Cargo.toml` and
   `package.json` but is **never imported** in `src/` — no third mock needed.
2. **The seams already exist.** `mockTransport` (`src/lib/transport.ts:256`)
   proves the scripted-patch pattern; `session.svelte.test.ts` proves the
   reducer accepts fixture snapshots; the play page consumes nothing but the
   `Transport` interface + `ipc.ts` wrappers. The browser harness mocks at the
   *same* boundaries the unit tests already mock at.
3. **Deterministic and display-free.** Headless Chromium needs no Xvfb, no
   WebKitGTK, no compiled app bundle — the CI job needs Node + pnpm only
   (browser binaries cached). Contrast `gui_smoke.sh`’s 45s boot budget and
   xvfb dance: needed for a boot check, wrong tool for 20+ scenarios.
4. **Two real browser contexts + device emulation are first-class** in
   Playwright (`browser.newContext()` ×2, `viewport: 390×844` per-file).
   tauri-driver can do neither multiple instances of one app nor mobile
   viewports.
5. **WebKitGTK automation is the known-bad path** (§2.2).

What this lane deliberately does **not** prove: production-bundle equivalence
(the alias changes the bundle, see §3.1 for why that is accepted), WebKitGTK/
WebView2 rendering fidelity, and Rust authority logic (already cargo-owned).

### 2.2 Alternatives considered

| Option | Verdict | Why |
|---|---|---|
| (b) `tauri-driver` + WebDriverIO against the real binary under xvfb | **Rejected for v1** | Linux rendering is WebKitGTK: no CDP attach (tauri#15934, open), `tauri-driver` ↔ WebKitWebDriver capability negotiation is actively breaking (tauri#15415, BiDi strip bug, reproduced 100% on Ubuntu 22.04/24.04), Ubuntu LTS ships pre-2.46 `webkit2gtk-driver`. Requires building the app, xvfb, and a driver chain in CI — the flakiness profile on the exact platform CI uses. The newer `@wdio/tauri-service` (embedded driver) still requires adding two Tauri plugins (`tauri-plugin-wdio`, `tauri-plugin-wdio-webdriver`) as **Rust deps** — heavier intrusion than any option here. |
| (a→variant) Runtime injection: `page.addInitScript` installs a fake `window.__TAURI_INTERNALS__` against the *production* `pnpm build` output | Deferred (tier-2 candidate) | Tests the exact shipped bundle, but couples the harness to `@tauri-apps/api` v2 internals (`__TAURI_INTERNALS__.invoke`, `transformCallback`, `runCallback`, callback registry) — an undocumented surface that can shift under a routine `pnpm update` of `@tauri-apps/api`, while the mocked-interface alias in §2.1 sits on the stable public API (`invoke`, `listen` signatures). If bundle-equivalence evidence is ever required, add it as one env-gated probe spec, not as the base layer. |
| (c) Hybrid per-repo conventions | **Adopted** | Tier 1 = §2.1 (default CI). Tier 2 = `PF2E_E2E_LAN=1` loopback lane: one real browser page driving the real `wsTransport` against the real Rust `lan` server (pattern already established by `src-tauri/tests/phase10_lan.rs`). Tier 3 = `gui_smoke.sh`, unchanged. |

### 2.3 Browser project

Chromium only (`chromium` project, `pnpm exec playwright install chromium`).
No production renderer is Chromium (Linux = WebKitGTK, macOS/iOS = WKWebView,
Windows/Android = WebView2-family), so cross-browser matrix buys fidelity
claims we cannot make anyway; screenshots are evidence, not pixel truth (§5).

## 3. Architecture

### 3.1 Harness mode: how the app detects it

One mechanism, config-only, no edits to `src/**`:

- `vite.config.js` — change the export from `defineConfig(async () => …)` to
  `defineConfig(async ({ mode }) => …)` and append:

  ```js
  resolve: mode === "harness" ? {
    alias: {
      "@tauri-apps/api/core":  fileURLToPath(new URL("./tests/e2e/harness/tauri-core.ts",  import.meta.url)),
      "@tauri-apps/api/event": fileURLToPath(new URL("./tests/e2e/harness/tauri-event.ts", import.meta.url)),
    },
  } : undefined,
  ```

- New package scripts (exact names, §6):
  - `"build:harness": "vite build --mode harness"`
  - `"e2e": "playwright test"`
  - `"e2e:live": "PF2E_E2E_LAN=1 playwright test --grep @live"`

Production paths are untouched: `pnpm dev`, `pnpm build`, `tauri dev`,
`tauri build` never pass `--mode harness`, and `gui_smoke.sh`’s `pnpm build`
unchanged. Vitest’s config needs no edit: its `test.include` is
`["src/**/*.test.ts"]` and e2e specs are `tests/e2e/*.spec.ts` — outside the
glob by construction. Vitest does not read the alias (mode-conditional)
and no existing unit test imports `@tauri-apps/api/*`.

Accepted tradeoff: the harness tests a **harness-mode bundle**, not the
byte-identical production bundle. Mitigations: the delta is exactly two module
substitutions at stable public boundaries (verified enumerable in §2.1.1);
`pnpm check` and all unit tests see un-aliased imports (svelte-check does not
consult Vite `resolve.alias`); real-binary boot remains gated by
`gui_smoke.sh`; the live lane (§3.4) exercises unmocked `transport.ts` WS code
against the real server.

### 3.2 The mock modules (bundled only in harness mode)

`tests/e2e/harness/tauri-core.ts` (side-effect module; installed once at import):

```ts
// Shape of the contract with the Playwright side. Declarative — this is the
// full wire protocol; implementation must match it exactly.
type Stub = { ok: unknown } | { error: string } | { queue: Stub[] };
interface Window {
  __PF2E_HARNESS_CONFIG__?: { responses: Record<string, Stub> };
  __pf2eHarness: {
    invocations: Array<{ cmd: string; args: unknown }>;   // append-only record
    listenerCount(event: string): number;                  // readiness probe
    emit(event: string, payload: unknown): void;           // patch/presence push
    reset(): void;                                         // between phases
  };
  __TAURI_INTERNALS__?: Record<string, never>;             // set to {} by this module
}
```

Behavior, point by point:

1. On import, the module sets `window.__TAURI_INTERNALS__ ??= {}`. This is what
   `createTransport()` (`src/lib/transport.ts:326`) checks, so harness pages get
   `ipcTransport()` — whose `listen`/`send` resolve to the mocked modules. The
   flag is a plain object; nothing in `src/` reads properties from it.
2. On import, the module installs `window.__pf2eHarness` (above).
3. `export async function invoke(cmd, args)`: appends to `invocations`; then
   resolves `window.__PF2E_HARNESS_CONFIG__.responses[cmd]` — `{ok}` → return
   value; `{queue}` → shift (empty queue ⇒ hard error); `{error}` → `throw new
   Error(error)`; **missing key ⇒ `throw new Error('harness: no stub for
   "<cmd>"')`** (fail fast: an unstubbed command is a fixture bug, the test
   turns red immediately instead of silently navigating an error path).
4. `tests/e2e/harness/tauri-event.ts` exports `listen(event, cb)` /
   `unlisten`-compatible handles; registrations are counted per event name;
   `window.__pf2eHarness.emit(event, payload)` synchronously dispatches to all
   listeners registered for that event. `emit()` records and no-ops.
5. `tests/e2e/harness/global.d.ts` provides the `Window` augmentation so the
   above type-checks under `pnpm check` (§3.5).

Why config-data + module-side install (and not closures via `addInitScript`):
`page.addInitScript` runs before every page script but can only transport
JSON-serializable data. Fixture *data* rides `addInitScript`; fixture *behavior*
(the invoke dispatcher, the event bus) is bundled code installed by the mock
module itself. There is no race: `addInitScript` → page module scripts → the
alias modules install `__pf2eHarness` before any route's `onMount` runs.

### 3.3 Playwright side: helpers and fixtures

`tests/e2e/helpers/harness.ts` (exact exported names — grep-verifiable §6):

- `installHarness(page, overrides?: Record<string, Stub>)` — seeds
  `BASE_RESPONSES` (the baseline every route's boot needs: `list_lenses` →
  `[]`, `list_campaigns` → one fixture campaign, `schema_version` → `12`,
  `llm_status` → `{ configured: false, … }`), merges `overrides`, and calls
  `page.addInitScript(cfg => { window.__PF2E_HARNESS_CONFIG__ = cfg; }, merged)`.
- `emitPatch(page, patch: PatchEvent)` / `emitPresence(page, ev: PresenceEvent)`
  — `page.evaluate` into `__pf2eHarness.emit("session:patch" | "session:presence", …)`.
- `waitForListener(page, event: string)` — polls `listenerCount(event) > 0`.
  **All gameflow specs must await this before their first emit** (the play
  page attaches listeners in `onMount`; emitting earlier silently drops patches
  and is the primary flake source this rule exists to kill).
- `invocations(page)` — returns the recorded `{cmd, args}` list;
  `expectIntents(page, kind, matcher)` — convenience assertion over the
  `session_intent` records (proves exact intent shapes, e.g. secret rolls).
- `shot(page, relPath: string)` — writes
  `tests/e2e/screenshots/<spec-stem>/<relPath>.png` and `testInfo.attach`es it
  (path convention §5). `fullPage: true` default for desktop passes.

`tests/e2e/helpers/session.ts` — shared gameflow drivers built on the above:
`hostViaLobby(page, { name, adventureSlug? })`, `joinViaLobby(page, { name,
ticket })`, `emitSnapshotFromGm(gmPage, playerPage, builderOpts)` (builds one
snapshot, emits to both contexts — interleaves `waitForListener`).

`tests/e2e/fixtures/` — typed builders, all importing types from `$lib/ipc`
and reusing the `makeSnapshot` shape from `src/lib/session.svelte.test.ts`:
`search.ts` (SearchHit/EntityDetail incl. a statblock), `table.ts` (XpBudgetResult),
`vault.ts` (Campaign/EntitySummary list), `settings.ts` (EvalPrompt/SuiteSummary),
`session.ts` (SessionInfo with fixed `pin: "A7K9M2"`, `lan_url:
"ws://192.168.1.42:4780/abc123"`, fixed session/peer ids `sess-e2e`,
`gm-peer-01`, `pl-peer-02`) **with monotonic timestamps** (`let ts = 1_700_000_000_000; export const nextTs = () => ++ts;` — chat `{#each}` is keyed
by `entry.ts` at `table/play/+page.svelte:348`, so duplicate timestamps are a
fixture bug class the builders make impossible).

### 3.4 Two-client architecture (both lanes)

**Deterministic lane (default CI).** One Playwright test, two
`browser.newContext()`s. The test script *is* the authority layer: it decides
which patches each side receives, then **asserts the intent log** on the
sending side and the **DOM** on the receiving side. Division of labor with
cargo: authority/secret-filtering semantics are server-proven in
`phase10_session.rs`/`phase10_lan.rs`; the harness proves (a) the UI emits
exactly `{kind:"roll_dice", expr, secret:true}` etc., and (b) given a patch
stream, GM and player DOMs diverge correctly. The harness **never** asserts
“player didn't receive the secret-roll patch” by simply not sending it —
vacuous. Instead S20 asserts the *positive witness* pair: GM DOM contains the
secret entry (emitted to GM context only), player DOM does not contain it while
containing a subsequent public entry, and the GM-side intent log shows
`secret: true`.

**Live lane (`PF2E_E2E_LAN=1`).** One real Chromium page as a **LAN player**;
GM duties are performed by a Rust helper binary, not a browser:

- `src-tauri/examples/e2e_host.rs` (~150 lines, default features incl. `lan`):
  builds `SessionService` exactly like `tests/phase10_lan.rs:33` (pinned
  `SessionMeta`, `--pin A7K9M2`), binds `TcpListener 127.0.0.1:0`, spawns
  `lan::serve`, prints the port on stdout, then loops over **NDJSON on stdin**:
  each line `{"intent": <Intent>}` is applied to the service as the GM;
  emitted `PatchEvent`s stream to stdout as NDJSON so the Playwright side can
  synchronize on effects instead of sleeping. Examples compile under
  `cargo test` (build-time check only; never executed, never bundled).
- `tests/e2e/server/liveHost.ts` — spawns `cargo run --example e2e_host` (only
  when the env gate is set), parses the port line, exposes
  `gmIntent()`/`nextPatch()` helpers over the pipes.
- The browser page joins via the **real unmocked path**:
  `/table/play?name=Alice&host=ws://127.0.0.1:<port>/lan-test&pin=A7K9M2` —
  `createTransport()` picks `wsTransport` (`?host=`), no alias mock involved
  for the transport (this is the one place `installHarness(page, { tauri:
  false })` omits the `__TAURI_INTERNALS__` flag; IPC stubs for
  `list_lenses`/`list_campaigns` still apply since `+layout.svelte` boots them).

### 3.5 Type-safety of fixtures and harness code

Fixtures only earn their keep if they break the build when `ipc.ts` types
change. Root `tsconfig.json` extends `./.svelte-kit/tsconfig.json`; per the
comment in that file, the include list is re-declared verbatim and extended
with `"tests/e2e/**/*.ts"`. Result: `pnpm check` (svelte-check) type-checks
harness + fixtures + specs — including `@playwright/test` imports (types-only,
resolvable from devDependencies). Standing guardrail “0 errors / 0 warnings”
therefore covers the harness. **Escape hatch (documented, not expected yet):**
if svelte-check proves noisy on Playwright idioms, replace the include change
with a separate `tsconfig.e2e.json` + `pnpm exec tsc -p tsconfig.e2e.json
--noEmit` step inside the `e2e` npm script and drop the include line. The
decision is recorded in §9 rollout notes either way.

### 3.6 Where things live (and where they don't)

- Runner home: `tests/e2e/` (specs, helpers, fixtures, harness mocks).
  `scripts/` stays repo utility scripts (currently only `extract_seeds.py`).
- Playwright config: `playwright.config.ts` at the app root; `testDir:
  "tests/e2e"`; `webServer` command `pnpm build:harness && pnpm preview
  --port 4173 --strictPort`; `reuseExistingServer: !process.env.CI`; `workers:
  1` (screenshot paths and two-context choreography stay serial and boring);
  `forbidOnly: !!process.env.CI`; `retries: process.env.CI ? 1 : 0` — retry
  *once* in CI, and a test that only passes on retry must be treated as a bug
  filed in the PR, not accepted.
- Screenshot/store root: `tests/e2e/screenshots/` (gitignored), HTML report
  `playwright-report/` (gitignored). Both uploaded as CI artifacts (§5).
- No production source file is added or modified. The complete list of
  edits to existing files is in §6: `vite.config.js`, `tsconfig.json`,
  `package.json`, `.gitignore`, `.github/workflows/build.yml`, `README.md`.

## 4. Scenario list

Numbering: `S##` = deterministic lane (default CI); `L#` = live lane
(`PF2E_E2E_LAN=1`). Every scenario is one Playwright `test()` whose title
starts with its id (e.g. `test("S15 player-joins …", …)`), so each is
grep-verifiable. Screenshot paths are relative to
`tests/e2e/screenshots/`. Unless stated otherwise, every screenshot step ends
with the matching `shot(page, …)` call. “Assert intents” means
`expectIntents(page, ...)` against the recorded invocation log.

### Group A — reference surfaces (`tests/e2e/reference.spec.ts`)

**S01 search-results-filter** — Route `/`. Stubs: `schema_version` → `12`,
`search` → 3 fixture hits (one each `deity`, `spell`, `creature` with
`source: "fts"`). Assert `schema v12` badge, 3 result rows, titles from
fixtures. Select type filter “Spells” (client-side filter) → exactly 1 row,
spell title. Trigger a new search whose stub queue is empty → override stub
`search` → `{ok: []}` and re-query → zero rows. Shots:
`reference/01-search-results.png`, `reference/02-search-type-filter.png`,
`reference/03-search-empty.png`.

**S02 entity-statblock** — Route `/entity/lewisian:deity-hosts` (fixture id)
then `/entity/missing:id`. Stubs: `get_entity` queue → `[{ok:
fixtureEntityWithStatblock}, {ok: null}]`. Assert title + `Statblock` card
(visible `AC` label) for the hit; not-found state for the null. Shots:
`reference/04-entity-statblock.png`, `reference/05-entity-missing.png`.

**S03 encounter-math** — Route `/encounter`. Stubs: `xp_budget` queue: two
results (moderate: `xp_budget: 80`; extreme: `xp_budget: 160`);
`creature_xp` → `{ok: 40}`. Assert the budget number `80` renders; switch
difficulty to Extreme → assert `160` and that a second `xp_budget` invocation
was recorded with `{difficulty: "extreme"}` args. Shots:
`reference/06-encounter-moderate.png`, `reference/07-encounter-extreme.png`.

**S04 aliases** — Route `/aliases`. Stub `lookup_alias` queue → `[{ok:
[fixtureAliasHit]}, {ok: []}]`. Assert legacy→remaster mapping row renders;
second query yields empty state. Shots: `reference/08-aliases.png`.

**S05 miracles** — Route `/miracles`. Stub `lookup_miracle` → `{ok:
[fixtureMiracleHit]}`. Assert spell name + reference cells render. Shots:
`reference/09-miracles.png`.

**S06 about** — Route `/about`. No stubs beyond base. Assert page renders and
contains the licensing/sources copy. Shots: `reference/10-about.png`.

### Group B — vault (`tests/e2e/vault.spec.ts`)

**S07 vault-list** — Route `/vault`. Stubs: `list_campaigns` → 1 campaign
(“Millbrook”), `list_entities` → 2 rows. Assert campaign select shows
Millbrook, entity rows render. Shots: `vault/01-list.png`.

**S08 vault-editor-validation** — Route `/vault/<fixture-entity-id>`.
Stubs: `get_entity` → entity fixture; `validate_statblock` → `{ok: {valid:
false, errors: ["name required"], warnings: []}}`; `update_entity` → `{error:
"validation failed: name required"}`. Actions: open EntityEditor, blank the
title, save. Assert the error string is visible and `update_entity` was
invoked exactly once with the expected `id`. Shot: `vault/02-editor-validation.png`.

**S09 vault-editor-save** — Same entry; stubs `update_entity` → `{ok: {id,
file_path}}`, `list_entities` re-stubbed post-save. Assert save success surface
(no error banner) and recorded invocation args contain the patched title. Shot:
`vault/03-editor-saved.png`.

### Group C — settings / LLM-off (`tests/e2e/settings.spec.ts`)

**S10 llm-off** — Route `/settings/llm`. Base stub `llm_status` →
`{configured: false, provider: null, model: null, key_present: false}`.
Assert the not-configured guidance renders; **no** `llm_configure` invocation
occurs until a form submit. Shot: `settings/01-llm-off.png`.

**S11 eval-results** — Route `/settings/eval`. Stubs: `eval_load_suite` → 3
fixture prompts; `eval_run` → `SuiteSummary{total: 3, passed: 2, failed: 1}`
with one `RunResult.failures = ["expected tool_called roll_dice"]`. Click Run.
Assert 2/3 badge and the failure string visible. Shot:
`settings/02-eval-results.png`.

**S12 chat-gated** — Route `/chat` with LLM-off stub. Assert the
“not configured / set up in Settings” gate copy renders and **no** `llm_chat`
invocation is possible (input disabled or absent). Shot:
`settings/03-chat-gated.png`. (LLM token streaming: non-goal, §1.)

### Group D — table lobby (`tests/e2e/table-lobby.spec.ts`)

Context isolation note: Playwright gives each test a fresh context (empty
localStorage), so the lobby always boots without `pf2e.playerName` prefs —
deterministic identity.

**S13 lobby-no-campaign** — Route `/table`. Stubs: `list_campaigns` → `{ok:
[]}`. Assert “Create a campaign in the Vault first” copy and the disabled Host
button. Shot: `table-lobby/01-no-campaign.png`.

**S14 lobby-host-invite** — Full host flow through the UI: base stubs +
`session_host` → `{ok: fixtureSessionInfoGm}` (pin `A7K9M2`, lan_url
`ws://192.168.1.42:4780/abc123`, p2p_ticket `pf2e-ticket-…`). Fill identity,
session name “Widow’s Lamp — Session 1”, adventure `widows-lamp`, submit.
Assert recorded `session_host` args `{campaignId, name, adventureSlug:
"widows-lamp"}`; auto-navigation to `/table/play` occurs; then `page.goto("/table")`
(the real “back to lobby to share invites” flow) → invite panel visible with
all three of PIN / LAN URL / ticket rendered verbatim, and “Enter Table →”
link present. Shots: `table-lobby/02-invite.png`.

**S15 lobby-join-error** — Stubs: `session_join_peer` → `{error: "no such
ticket"}`. Attempt ticket join. Assert inline error shows the message, no
navigation occurred, `session_join_peer` invoked once. Also switch to the LAN
tab and screenshot both tab states. Shots: `table-lobby/03-join-error.png`,
`table-lobby/04-join-lan-tab.png`.

### Group E — two-context gameflow (`tests/e2e/gameflow.spec.ts`)

All scenarios use `hostViaLobby`/`joinViaLobby` helpers; fixed ids from
`fixtures/session.ts` (`sess-e2e`, `gm-peer-01` “GM Dana”, `pl-peer-02`
“Alice”). Both contexts run `installHarness`; the test script is the authority
(§3.4). `waitForListener(page, "session:patch")` precedes every first emit.

**S16 gm-empty-table** — GM hosts via lobby (S14 stubs), lands on
`/table/play`; emit snapshot (players: GM only, empty chat/scene/handouts);
assert GM badge, all 6 tabs including 🔧 GM, empty-chat hint, “No players yet”
in Party. Shot: `gameflow/01-gm-empty-table.png`.

**S17 player-joins** — Player context joins via lobby ticket
(`session_join_peer` → `{ok: fixtureSessionInfoPlayer}`); emit
`peer_joined(Alice, via: "lan")` + full snapshot (players: GM + Alice) to
*both* pages. Assert: GM Party tab shows Alice connected; player page shows
“Player” badge, **no** GM tab, presence/party list has both names. Shots:
`gameflow/02-player-joined-gm-pov.png`, `gameflow/03-player-joined-player-pov.png`.

**S18 load-adventure** — GM opens GM tab, clicks “Load Widow’s Lamp”. Assert
GM intents include `{kind: "load_adventure", slug: "widows-lamp"}`. Emit
`adventure_loaded` + `scene_set` (fixture grid/tokens) + snapshot with
`handouts: {letter: {…, visible_to: "gm"}}` to both. Assert adventure badge
`📖 widows-lamp` on both pages; GM handouts tab lists `letter` with “Reveal to
Players”; player handouts tab = empty state. Shots:
`gameflow/04-adventure-loaded-gm.png`, `gameflow/05-adventure-loaded-player.png`,
`gameflow/06-handouts-hidden-player.png`.

**S19 first-roll** — Player quick-rolls `1d20+7` (dice input, Enter). Assert
player intents include exactly `{kind: "roll_dice", expr: "1d20+7"}` (secret
falsy). Emit `chat_appended` roll entry (RollRecord total 24, deterministic
`rolls: [17]`, unique ts) to both. Assert roll line visible in both chat logs
with breakdown. Shot: `gameflow/07-first-roll-player.png`,
`gameflow/08-first-roll-gm.png`.

**S20 secret-gm-roll** — GM drawer dice: type `2d6+3`, roll. Assert GM intents
include `{kind: "roll_dice", expr: "2d6+3", secret: true}`. Emit the secret
roll entry **to the GM page only**; then emit a public GM chat (“The door
clicks open.”) to both. Assert: GM chat contains the secret roll AND the
public line; player chat contains the public line and **not** the secret roll
text/total (ordering witness per §3.4). Shots:
`gameflow/09-secret-roll-gm.png`, `gameflow/10-secret-roll-player-pov.png`.

**S21 handout-reveal** — GM handouts tab, click “Reveal to Players”. Assert GM
intents include `{kind: "reveal_handout", handout_id: "letter"}`. Emit
`handout_revealed` to both. Assert player handouts tab now lists `letter` (and
GM button state changes). Shots: `gameflow/11-handout-revealed-gm.png`,
`gameflow/12-handout-revealed-player.png`.

**S22 initiative-advance** — GM adds Alice + self via the Initiative tab’s
“— Add to initiative —” control, clicks “▶ Next Turn”. Assert intents `{kind: "set_initiative",
order}` then `{kind: "advance_initiative"}`; emit `initiative{active_idx: 1,
round: 1}` to both; assert active row highlight moved on both pages. Shot:
`gameflow/13-initiative-advanced.png`.

**S23 end-session-teardown** — Register `page.on("dialog", d => d.accept())`
on the GM page (native `confirm()` at `table/play/+page.svelte:115`), click
“End Session” in the GM tab. Assert intents include `{kind: "end_session"}`.
Emit `session_ended` to both. Assert both pages render the drained post-session
surface (badge sans snapshot, empty panels) — matching the reducer contract
(`session.svelte.ts:27`). Shot: `gameflow/14-session-ended.png`.

**S24 resume-gap (documented, xfail-style)** — `session_load` has an IPC
wrapper (`ipc.ts:451`) but the lobby’s `handleLoad` is an unwired stub
(`table/+page.svelte:108–121`: sets an error string, no session picker). The
harness does **not** paper over this: S24 asserts the current truth — after
S23, returning to `/table` shows host/join only, and `session_load` is never
invoked from the UI. When a resume UI lands (future phase), this scenario is
replaced by a real resume: `session_load` stub → SessionInfo → emit snapshot →
play surface repopulated. No shot; assertion-only.

### Group F — resilience (`tests/e2e/resilience.spec.ts`)

**S25 error-banner** — On a hosted play surface, emit `{kind: "error",
message: "authority: only the GM may reveal handouts"}` to the player page.
Assert red banner with the message; dismiss ✕ clears it. Shot:
`resilience/01-error-banner.png`.

**S26 ws-connection-failed** — Special harness config (`tauri: false`, i.e. no
`__TAURI_INTERNALS__` flag): open `/table/play?name=Alice&host=ws://127.0.0.1:9/x`
(port 9 = discard, connection refused fast). `wsTransport` retries 3×
(500/1000/2000 ms backoff, `transport.ts:104`) then `failed`. Assert the
“Connection lost” banner + “Rejoin” button renders. Retry budget ≈ 3.5 s —
inside Playwright’s 5 s expect window. Shot: `resilience/02-connection-failed.png`.

### Group M — mobile viewport (`tests/e2e/mobile.spec.ts`)

`test.use({ viewport: { width: 390, height: 844 }, hasTouch: true })`.
One scenario, five stops:

**S27 mobile-pass** — (1) `/` with S01 stubs; (2) `/table` lobby with S14
stubs post-host (invite panel incl. PIN); (3) `/table/play` player POV (host
via helpers’ mobile variant: emit player snapshot directly after
`waitForListener`); (4) `/vault` with S07 stubs; (5) `/entity/<fixture-id>`
with S02 stubs. At each stop assert: bottom tab-bar visible (Search/Encounter/
Vault/Aliases/Miracles/Table), and
`document.documentElement.scrollWidth <= 390` (no horizontal overflow).
Shots: `mobile/01-home.png`, `mobile/02-lobby-invite.png`,
`mobile/03-play-player.png`, `mobile/04-vault.png`, `mobile/05-entity.png`.

### Live lane (`tests/e2e/live-lan.spec.ts`, all `test.describe` gated)

Every test is tagged `@live` and self-skips unless `PF2E_E2E_LAN=1` (same
pattern as `PF2E_NET_TESTS` in `phase10_p2p.rs:54`). Spawns
`src-tauri/examples/e2e_host.rs` via `tests/e2e/server/liveHost.ts` (§3.4).
Player page = real `wsTransport` (§3.4), so these specs never call
`installHarness` with the Tauri flag.

**L1 join-snapshot-chat** — Page loads `/table/play?name=Alice&host=…&pin=A7K9M2`.
Assert: welcome → connected badge (no error banner), snapshot renders (GM Dana
present in Party), send a chat → server-broadcast `chat_appended` renders.
Asserts the frontend speaks the §4.3 frame protocol against real Rust.

**L2 wrong-pin-rejected** — Same with `pin=ZZZZZZ`. Assert error frame → UI
error surface (and `welcome` never arrives).

**L3 secret-roll-filtering** — Stdin to e2e_host: GM secret roll intent, then
GM public chat. Assert the public chat entry renders; assert (with the public
entry as ordering witness) that no secret-roll entry ever renders.

**L4 gm-load-adventure** — Stdin: `{"intent": {"kind": "load_adventure",
"slug": "widows-lamp"}}`; assert player page shows the adventure badge after
the broadcast. Uses the same bundled content resolution as the session tests.

## 5. Screenshot storage, review, and CI integration

### Storage convention

- **Path:** `tests/e2e/screenshots/<spec-stem>/<NN>-<step>.png`, written by the
  `shot()` helper only (one call site → one writer rule). Exact paths per
  scenario are listed in §4.
- **Git:** `tests/e2e/screenshots/`, `test-results/`, and `playwright-report/`
  are added to `.gitignore`. Screenshots are **not committed** in v1. Review
  happens on the CI artifact, not in the repo.
- **CI artifacts:** the `e2e` job uploads with `if: always()`:
  - artifact `e2e-screenshots` ← `pf2e-companion/tests/e2e/screenshots/`
  - artifact `e2e-report` ← `pf2e-companion/playwright-report/` (every shot is
    also `testInfo.attach`ed, so the HTML report is the primary review surface;
    the bare PNG dir is the fallback for “show me gameflow/09 quickly”).
- Retention: workflow default. Reviewers open the HTML report artifact on any
  PR touching `src/routes/**` or `src/lib/**` — this is the human gate that
  replaces pixel baselines.

### Evidence-only policy (v1) and its revisit trigger

Screenshots are **captured, never asserted**. No `toHaveScreenshot`, no
baseline PNGs in git. Rationale: the point of v1 is *gameflow evidence*;
Chromium-AA/font deltas across runner images would mint false negatives from
day one, and the app’s own fonts load from the system stack
(`+layout.svelte:74`), so byte-stability is not ours to promise.

Revisit trigger: if (a) two consecutive CI runs on untouched code produce
byte-identical PNGs for every shot, and (b) at least one scenario has caught a
visual bug in review, then an opt-in baseline lane may be added behind
`PF2E_VRT=1` with baselines under `tests/e2e/baselines/` — as a new contract
amendment, not silently.

### CI integration

New job **`e2e`** appended to `.github/workflows/build.yml` (job name exact;
existing `test`/`desktop`/`ios`/`android` jobs unchanged):

- `runs-on: ubuntu-latest`, no `needs:` (runs parallel with `test`; it has no
  Rust dependency and must stay fast).
- Steps: `actions/checkout@v4` → `pnpm/action-setup@v4` (PNPM_VERSION 11) →
  `actions/setup-node@v4` (node 20, `cache: pnpm`,
  `cache-dependency-path: pf2e-companion/pnpm-lock.yaml`) →
  `pnpm install --frozen-lockfile` (wd `pf2e-companion`) →
  `actions/cache@v4` on `~/.cache/ms-playwright` keyed by
  `${{ runner.os }}-playwright-${{ hashFiles('pf2e-companion/pnpm-lock.yaml') }}` →
  `pnpm exec playwright install --with-deps chromium` → `pnpm e2e` →
  `actions/upload-artifact@v4` ×2 with `if: always()` as above.
- Determinism budget: no WAN (the harness server is `vite preview` on
  loopback; Playwright’s browser download is the only network and is cached);
  no display; no Rust toolchain; no `GUI_SMOKE`/`NET_TESTS`/`NET_WAN` vars.
- The `@live` lane is **not** a PR gate in v1. It runs locally via
  `PF2E_E2E_LAN=1 pnpm e2e:live`; wiring it into a nightly/manual workflow is a
  §9 rollout decision, deferred until it has run green locally for two weeks.

## 6. Exact files — grep-verifiable

New files (all paths relative to `pf2e-companion/`):

| File | Contents / acceptance anchor |
|---|---|
| `playwright.config.ts` | `testDir: "tests/e2e"`, `workers: 1`, webServer `pnpm build:harness && pnpm preview --port 4173 --strictPort` |
| `tests/e2e/harness/tauri-core.ts` | mock `invoke`; installs `window.__pf2eHarness`; sets `__TAURI_INTERNALS__`; throws `harness: no stub for` on unstubbed cmd |
| `tests/e2e/harness/tauri-event.ts` | mock `listen`/`emit`; per-event listener registry + counts |
| `tests/e2e/harness/global.d.ts` | `Window` augmentation (`__pf2eHarness`, `__PF2E_HARNESS_CONFIG__`, `__TAURI_INTERNALS__`) |
| `tests/e2e/helpers/harness.ts` | exports `installHarness`, `emitPatch`, `emitPresence`, `waitForListener`, `invocations`, `expectIntents`, `shot` |
| `tests/e2e/helpers/session.ts` | exports `hostViaLobby`, `joinViaLobby`, `emitSnapshotFromGm` |
| `tests/e2e/fixtures/search.ts` | `fixtureSearchHits`, `fixtureEntityWithStatblock` |
| `tests/e2e/fixtures/table.ts` | `fixtureXpModerate`, `fixtureXpExtreme`, `fixtureCreatureXp`, alias/miracle hits |
| `tests/e2e/fixtures/vault.ts` | `fixtureCampaign`, `fixtureEntitySummaries` |
| `tests/e2e/fixtures/settings.ts` | `fixtureEvalSuite` (3 prompts), `fixtureSuiteSummary` (2/1) |
| `tests/e2e/fixtures/session.ts` | `fixtureSessionInfoGm`, `fixtureSessionInfoPlayer`, `gmSnapshotBuilder`, `nextTs` |
| `tests/e2e/reference.spec.ts` | tests `S01`…`S06` |
| `tests/e2e/vault.spec.ts` | tests `S07`…`S09` |
| `tests/e2e/settings.spec.ts` | tests `S10`…`S12` |
| `tests/e2e/table-lobby.spec.ts` | tests `S13`…`S15` |
| `tests/e2e/gameflow.spec.ts` | tests `S16`…`S24` |
| `tests/e2e/resilience.spec.ts` | tests `S25`, `S26` |
| `tests/e2e/mobile.spec.ts` | test `S27` |
| `tests/e2e/live-lan.spec.ts` | tests `L1`…`L4`, all under `test.describe("@live …")` with `PF2E_E2E_LAN` skip |
| `tests/e2e/server/liveHost.ts` | spawns/communicates with the example binary (NDJSON stdio) |
| `src-tauri/examples/e2e_host.rs` | pinned-meta SessionService + `lan::serve` on `127.0.0.1:0`, NDJSON stdin intents / stdout patch stream |

Edits to existing files (complete list — anything not here is untouched):

| File | Edit |
|---|---|
| `vite.config.js` | `defineConfig(async ({ mode }) => …)` + `resolve.alias` block active only when `mode === "harness"` (§3.1) |
| `tsconfig.json` | re-declare base includes + add `"tests/e2e/**/*.ts"` (§3.5; escape hatch `tsconfig.e2e.json` documented there) |
| `package.json` | add scripts `build:harness`, `e2e`, `e2e:live`; add devDependency `@playwright/test` (§7) |
| `.gitignore` | add `test-results/`, `playwright-report/`, `tests/e2e/screenshots/` |
| `.github/workflows/build.yml` | append job `e2e` exactly as §5; no other job edited |
| `README.md` | “Develop” section gains one block: `pnpm build:harness` / `pnpm e2e` / `PF2E_E2E_LAN=1 pnpm e2e:live`, plus a link to this plan — same placement rule as the `PF2E_GUI_SMOKE` block added by CONTRACT-11 §6 |

Verification commands (must all grep): `grep -rn '"S1[6-9]\|"S2[0-4]' pf2e-companion/tests/e2e/gameflow.spec.ts`; `grep -n "@live" pf2e-companion/tests/e2e/live-lan.spec.ts`; `grep -n "mode === \"harness\"" pf2e-companion/vite.config.js`; `grep -n "build:harness" pf2e-companion/package.json`; `grep -n "^  e2e:" .github/workflows/build.yml` (repo root).

## 7. Dependencies to add

Exactly one:

| Package | Scope | Justification |
|---|---|---|
| `@playwright/test` | devDependency | The entire tier-1 runner: Chromium driver, two-context isolation, device emulation, screenshots, traces, HTML report. Downloads its browser via `pnpm exec playwright install chromium` (cached in CI, §5). |
| *nothing else* | | jsdom/Testing Library: not needed (`vitest` keeps its existing reducer-level role; the harness runs real browsers). WebdriverIO stack + `tauri-plugin-wdio{,-webdriver}`: rejected (§2.2) — would add two **Rust** plugins and a broke-on-LTS driver chain. `pixelmatch`/snapshot libs: non-goal (§5 evidence-only). `cross-env`: not needed; harness mode is `--mode harness` (cross-platform by Vite convention) and the only inline env var (`PF2E_E2E_LAN=1` in `e2e:live`) targets unix dev shells, same assumption class as `gui_smoke.sh`. |

`@playwright/test` version: latest stable at implementation time, pinned by
`pnpm-lock.yaml` like every other devDependency. No `playwright` (library)
transitive pinning beyond what `@playwright/test` brings.

## 8. Risks + mitigations

1. **Renderer divergence (Chromium ≠ WebKitGTK/WKWebView/WebView2).**
   Screenshots may not match what a Linux desktop user sees pixel-wise.
   *Mitigation:* evidence-only policy (§5); `gui_smoke.sh` stays the real-binary
   boot gate; if a rendering-fidelity gap ever bites, the deferred
   `__TAURI_INTERNALS__` runtime-injection probe (§2.2) or a wdio embedded-driver
   spike is a *new* contract, not a silent scope creep.
2. **Race: emitting patches before the play page has registered listeners**
   (`onMount` → `transport.onPatch`). *Mitigation:* the `waitForListener` rule
   (§3.3) is a stated precondition of every gameflow scenario; the helper polls
   `listenerCount`, never sleeps; reviewers can grep that `emitPatch` call sites
   in `gameflow.spec.ts` are preceded by `waitForListener`.
3. **Fixture timestamp collisions.** Chat `{#each}` is keyed by `entry.ts`
   (`table/play/+page.svelte:348`); duplicate ts = broken rendering that would
   look like an app bug. *Mitigation:* `nextTs()` monotonic counter in
   `fixtures/session.ts`; builders cannot hand-write `ts`.
4. **Native `confirm()` on End Session** (`table/play/+page.svelte:115`).
   Playwright auto-dismisses unhandled dialogs, which would make the click a
   silent no-op. *Mitigation:* S23 registers `page.on("dialog", accept)`; the
   failure mode without it is loud (the `end_session` intent assertion fails),
   never green-by-accident.
5. **SPA deep-links under `vite preview`.** Routes like `/entity/<id>` rely on
   the `index.html` fallback. If preview mis-serves them, S02/S05/S27 fail at
   navigation. *Mitigation:* 12.0 includes a trivial smoke assertion that
   deep-link goto works in preview; fallback plan: enter via `/` and navigate
   client-side (`goto` is already how the app moves).
6. **`pnpm check` scope growth** (tsconfig include for `tests/e2e`).
   Playwright idioms might trip svelte-check’s zero-warning bar.
   *Mitigation:* documented escape hatch (§3.5): split tsconfig +
   `tsc -p --noEmit`. Either way fixtures stay type-checked — seam drift
   between fixtures and `ipc.ts` types is the classic silent-killer of mocked
   harnesses and is explicitly not negotiable.
7. **Automerge/sync timing in the live lane.** Real convergence has no fake
   clock. *Mitigation:* all L-scenario waits are bounded `expect` polls with
   ordering witnesses (public-after-secret pattern in L3); no sleep-based
   synchronization; stdio NDJSON from `e2e_host.rs` lets the test subscribe to
   effects.
8. **CI cost/flake pressure.** Browser download (~150 MB) per run.
   *Mitigation:* `actions/cache` on `~/.cache/ms-playwright` (§5); `workers: 1`
   avoids port/order entropy; `retries: 1` in CI with the pass-on-retry = file-a-bug
   rule (§3.6).
9. **Two-context bookkeeping bugs in the harness itself** (patches emitted to
   the wrong page). *Mitigation:* helpers `emitSnapshotFromGm(gmPage,
   playerPage, …)` take both pages as named parameters — call sites read
   wrongness aloud; S17–S22 each assert DOM on **both** sides so a misfire
   fails the test instead of corrupting evidence.
10. **Fixture rot when content/routes change.** A route gains a new IPC call →
    `harness: no stub for "<cmd>"` fails the spec immediately (§3.2.3). This is
    the desired behavior: the harness must fail loudly, never fall through to
    an accidental error-state screenshot being accepted as evidence.

## 9. Phased rollout — each step independently green

Contract amends itself only forward: a step lands as one PR, CI green with the
new `e2e` job, before the next begins.

- **12.0 — Skeleton + reference lane.** §7 dep; `vite.config.js`/`tsconfig.json`/
  `.gitignore`/`package.json` edits; `playwright.config.ts`; `harness/*`,
  `helpers/harness.ts`; `fixtures/{search,table,vault,settings}.ts`;
  `reference.spec.ts` (S01–S06); CI job `e2e` (uploads artifacts); README
  block. *Green gate:* `pnpm e2e` passes locally and in CI; `pnpm test`,
  `pnpm check` (0/0), `cargo test --all-features` unchanged. Includes the
  SPA-deep-link smoke assertion (risk 5).
- **12.0 landed** (date: 2026-10-14). Verified locally: `pnpm e2e` 6/6
  (S01–S06, chromium), `pnpm test` 86/86, `pnpm check` 0/0, `pnpm build`
  clean (production bundle grep-verified free of harness code), all ten
  `reference/*.png` shots captured. Deviations:
  - tsconfig escape hatch: **no** — `tests/e2e/**/*.ts` rides the re-declared
    include; svelte-check is quiet on the Playwright idioms. One extra
    `compilerOptions.types: []` override was added to tsconfig.json: the
    generated base tsconfig's `"types": ["node"]` is unresolvable under the
    pnpm strict store (pre-existing 1-warning at tsconfig.json:1:1); the
    override takes check from 0E/1W to 0E/0W. The one Node-global user in a
    checked file (vite.config.js `process`) already had `@ts-expect-error`;
    the new `node:url` import carries one too, same idiom.
  - SPA fallback needed: **no** — `vite preview` serves deep links
    (`/entity/lewisian%3Adeity-hosts` direct `page.goto` in S02) fine.
  - global.d.ts omits `__TAURI_INTERNALS__` from the `Window` augmentation:
    `transport.ts`/`transport.test.ts` carry `@ts-expect-error` directives
    that require the property to stay untyped; declaring it orphaned those
    directives (check errors). `tauri-core.ts` sets the flag via a local
    cast — runtime behavior per §3.2 point 1. 12.1/12.2 implementers: do
    **not** add `__TAURI_INTERNALS__` to the interface.
  - Wire-protocol addition: config carries optional `tauri?: boolean`
    (default true) so §3.4's `installHarness(page, { tauri: false })` can
    leave the flag unset for WS/live lanes. Mock-side exported `emit()`
    records to a module-local array (nothing in src/ imports it).
  - S02's "visible `AC` label" assertion adapted to the real deity-shaped
    Statblock component (no AC row exists): asserts the `.statblock` card,
    its "stat block" kind header, and the "Divine attribute" label.
  - S02's second leg navigates client-side (synthetic anchor click) instead
    of a second `page.goto`: a full navigation re-runs the init script and
    re-seeds stub queues (this also bites later phases — for same-page
    flows, navigate client-side or re-stub via `page.evaluate` on
    `window.__PF2E_HARNESS_CONFIG__.responses`).
  - Playwright pinned: `@playwright/test` 1.64.0 (pnpm-lock), Chromium
    headless shell v1248.
- **12.1 — Vault + settings.** `vault.spec.ts` (S07–S09), `settings.spec.ts`
  (S10–S12). Pure scenario addition.
- **12.1 landed** (2026-10-14). Verified locally: `pnpm e2e` 12/12
  (S01–S12, chromium), `pnpm test` 86/86, `pnpm check` 0/0; all six
  `vault/*.png` + `settings/*.png` shots captured. Fixture exports added
  (additive only): `fixtureVaultEntity` (vault.ts), `fixtureRagStatsEmpty` +
  `fixtureLlmConfigured` (settings.ts). Deviations vs §4:
  - **S08 flow order** — the EntityEditor save path never invokes
    `validate_statblock` (that command is an LLM-agent tool; referenced only
    in the chat system prompt). Validation is client-side first, twice over:
    (1) a fully blank title never reaches `handleSave` — the native
    `required` attribute on #title-input swallows the submit; (2) a
    whitespace-only title passes `required` and hits the JS gate
    (`"title is required"`, `role="alert"`, zero IPC). The scenario is
    therefore two legs: whitespace title → client gate asserted with
    `update_entity` count 0, then a real edit with `update_entity` stubbed
    `{error}` → error string visible and `update_entity` invoked exactly
    once with the expected `{id, patch.title}`. Also asserts
    `validate_statblock` count 0 across both legs.
  - **S09 post-save target** — on save success EntityEditor navigates
    client-side to `/entity/<id>` (not back to /vault), so `list_entities` is
    never re-invoked; instead `get_entity` is stubbed as a two-entry queue
    (editor load → post-save detail render). Asserts URL change, renamed h1,
    zero `.err`, and the full strict patch shape.
  - EntityEditor lens truth: the lens select initializes to
    `lensState.active` ("lewisian" default), so saving a lens-less entity
    emits `patch.lens: "lewisian"` — asserted as the true behavior.
  - **S11 needs `llm_status` configured** — /settings/eval hard-disables
    "Run suite" while unconfigured (button `disabled` + `runSuite`
    early-return), so "click Run" is unreachable under the LLM-off fixture.
    S11 overrides `llm_status` → `fixtureLlmConfigured`; `eval_run` remains
    fully stubbed. S10/S12 stay LLM-off as planned.
  - S07's "campaign select shows Millbrook" is real twice over: the layout's
    topbar `select[aria-label="Active campaign"]` gets value `millbrook`
    (auto-selected first campaign) and the vault page's `.camp.active` button
    renders it; both asserted, plus the `Entities · millbrook` section header
    and entity-row hrefs/type chips.
- **12.2 — Table.** `helpers/session.ts`, `fixtures/session.ts`,
  `table-lobby.spec.ts` (S13–S15), `gameflow.spec.ts` (S16–S24),
  `resilience.spec.ts` (S25–S26). This is the evidence centerpiece; PR
  description links the HTML report artifact for human review of the gameflow
  strip.
- **12.2 landed** (2026-10-14). Verified locally: `pnpm e2e` 26/26 in one
  pass (S01–S26, chromium; green on first run, no retries), `pnpm test`
  86/86, `pnpm check` 0/0, all twenty §4-listed shots under
  `tests/e2e/screenshots/{table-lobby,gameflow,resilience}/`, grep self-check
  32 `waitForListener` sites ≥ 25 emit call sites in `gameflow.spec.ts`.
  Files are exactly the five §6 names for this phase; no `src/**` edits, no
  dependency changes.
  Deviations vs §4 (all asserted-as-current-truth, commented inline, and
  evidencing the real wiring — none paper over app behavior):
  - **S14 invite panel unreachable in the current app.** §4 wanted
    host → goto `/table` → invite panel with PIN/LAN/ticket verbatim +
    "Enter Table →". Impossible as wired: `myTicket`/`myLanUrl` live in the
    lobby component's local `$state` and are only ever set inside
    `handleHost`, which immediately `goto("/table/play")`s; landing on and
    then leaving `/table/play` fires that page's `onDestroy` →
    `resetSession()`, erasing the `sessionState.info && connected` gate the
    invite panel also needs; nothing invite-related persists (localStorage
    holds only `pf2e.playerName`/color). S14 asserts the true contract
    instead: full host flow through the UI, exact `session_host` args
    `{campaignId:"millbrook", name:"Widow's Lamp — Session 1",
    adventureSlug:"widows-lamp"}`, auto-navigation, GM badge + peer prefix
    persisted on the play surface, then (client-side tab-bar return) the
    documented truth that no invite/resume affordance renders. Shot
    `table-lobby/02-invite.png` intentionally shows that post-host lobby —
    restore §4's verbatim assertions when a share/resume surface lands.
    Bonus finding the shot proves: player-name prefs do **not** survive the
    lobby remount (`onMount` awaits `refreshCampaigns()` before reading
    localStorage while the `$effect` writes prefs first) — app-level race,
    out of scope here, worth a follow-up.
  - **S26 — the planned "Connection lost + Rejoin" banner is unreachable
    from a cold start.** `wsTransport.doConnect()` rejects the initial
    `connect()` promise on the first refused socket (`onerror`,
    transport.ts:167), so the play page's `init()` catch shows the
    redirecting `.error-banner` (role="alert") and `goto("/table")`s after
    2 s — long before the 500/1000/2000 ms retry budget could reach the
    `failed` state at ~3.5 s. The failed+Rejoin surface only exists for an
    established-then-dropped connection (12.3 live lane should cover it:
    host drops mid-session). S26 pins the true cold-start surface: the
    redirecting banner, then arrival on the lobby's host/join panels.
    Type seam: `HarnessOverrides = Record<string, Stub> & { tauri?: boolean }`
    cannot express §3.4's `{ tauri: false }`; S26 uses one localized, commented
    assertion (`as HarnessOverrides`) rather than editing the 12.0 helper —
    12.3 adds more `{tauri:false}` callers and should loosen the signature
    properly (e.g. widened index signature + internal narrowing cast).
  - **S16 "players: GM only" vs "No players yet".** The party empty-hint
    renders only when the players map is *empty*, but the real host's roster
    always includes the GM (`service.rs host()` peer_joined()s the GM at
    session start) — so §4's two S16 fixtures conflict. S16 covers both
    render states in one test: empty roster → hint + zero party cards
    (shot), then the realistic GM-only roster → GM party card, hint gone.
    Observation: the hint is therefore unreachable in production today.
  - **S20 "GM drawer".** The floating overlay drawer (`showGmDrawer`) has no
    opener anywhere in the current markup. The reachable secret-roll surface
    is GM tab → Dice sub-tab → custom input → "Roll (Secret)" — S20 drives
    that and asserts `{expr:"2d6+3", secret:true}` in the GM intent log, the
    GM-only DOM, and the player's public-only DOM (positive witness pair).
  - **S22 no optimistic order.** The UI computes each `set_initiative` from
    the unchanged snapshot (no local echo), so bare "add Alice, add GM"
    emits two single-entry orders. S22 emits the server-style `initiative`
    echo patch after each Add (the loop a live server creates); the second
    intent then carries the cumulative `[pl-peer-02, gm-peer-01]` order
    §4 asserts.
  - **S15 error surface**: the lobby's inline error is `<p class="err">`
    with no `role="alert"` (unlike the play surface's banners which are
    role'd) — assertions target `.err`.
  Selector/flow knowledge for 12.3:
  - Play tab buttons: `.play-tabs [role="tab"]`, accessible names exactly
    "Chat+Dice|Initiative|Party|Scene|Handouts|GM" on desktop — but at
    <480 px the `.lbl` span is `display:none`, which strips the accessible
    name; mobile (S27) must select tabs by index/`aria-selected`, not name.
  - `.dice-quick input` matches TWO elements (the Secret checkbox is an
    input) — the quick-roll box is `.dice-quick
    input[placeholder="1d20+7"]`.
  - GM sub-tabs are `.gm-drawer-full .drawer-tabs [role="tab"]` ("Chat
    Log|Dice|Tools"); overlay drawer has no opener.
  - Lobby form fields are wrapping-label addressable: `getByLabel("Name",
    {exact:true})`, `"Session Name"`, `"Adventure (optional)"`, `"Ticket"`,
    `"Host LAN URL"`, `"Table PIN (6 chars)"`; adventure selectOption by
    value `"widows-lamp"`.
  - `hostViaLobby`/`joinViaLobby` and `emitSnapshotFromGm(gm, player|null,
    opts)` are in `tests/e2e/helpers/session.ts`; both drivers return only
    after the URL is `/table/play` *and* patch/presence listeners are
    registered, so the §8.2 rule is structural, not ceremonial.
  - Live-lane pages (`tauri:false`, direct `/table/play?…host=`): do not use
    `waitForListener`/emit — wsTransport never registers with the mock
    registry; synchronize on the `welcome`/connected state and server
    stdout instead. Reduction-side dedupe guards mean emitted chat entries
    need the `nextTs()` builder (`fixtures/session.ts`), never hand-written
    `ts`.
- **12.3 — Mobile + live lane.** `mobile.spec.ts` (S27);
  `src-tauri/examples/e2e_host.rs`, `server/liveHost.ts`, `live-lan.spec.ts`
  (L1–L4, `@live`, env-gated). CI wiring of the live lane is deferred per §5
  (two weeks of local greens first); the example binary still compiles under
  default `cargo test` from the moment it lands.
- **12.3 landed** (2026-10-14). Verified locally: `pnpm e2e` one pass — 27
  passed (S01–S27, chromium), 4 skipped (`@live` self-gates), nothing
  green-on-retry; `PF2E_E2E_LAN=1 pnpm e2e:live` L1–L4 4/4 (~11 s with the
  example cached — first spawn compiles `e2e_host` via cargo, hence liveHost's
  3-min ready bound); `pnpm test` 86/86; `pnpm check` 0/0;
  `cargo test --all-features` 0 failures; all five `mobile/*.png` shots
  captured. Files are exactly the four §6 names for this phase plus the two
  edits 12.2's note pre-authorized (the `HarnessOverrides` loosening and the
  S26 cast removal); no `src/**` edits, no dependency changes.
  Deviations vs §4 (all asserted-as-current-truth, commented inline, and
  evidencing the real wiring — none paper over app behavior):
  - **S27 caught a real app bug: every route has a 32 px horizontal overhang
    at viewports < 792 px.** +layout.svelte's `main` rule (width:100% with
    content-box sizing — there is no box-sizing reset anywhere in the app —
    plus `padding: 1rem 1rem 5rem`) makes main's border-box exactly
    viewport+32 wide, so `documentElement.scrollWidth` is 422 at 390 px on
    EVERY route and the rightmost ~16 px of content is clipped on a real
    phone. Diagnosed per-element (bounding-rect walk over every node: `main`
    is the only offender; all route children fit the padded column). §4's
    literal `scrollWidth <= 390` is therefore unreachable and `src/**` is
    outside this phase's §6 edit list, so S27 asserts the stripped-down
    truth: `scrollWidth <= 390 + KNOWN_LAYOUT_OVERHANG_PX (=32)`, named and
    documented in the spec — any route-level overflow beyond the fixed chrome
    overhang still fails the scenario. **Follow-up (app bug, file it):** give
    +layout.svelte's `main` `box-sizing: border-box` (or drop the `width:100%`
    — `.app` is a column flex, stretch already sizes it) and restore the
    strict `<= viewport` invariant in `mobile.spec.ts`.
  - **L1's "send a chat" pins intent shape, not wire delivery.** The play
    page's outbound chat goes through ipc.ts's `invoke`, never
    `transport.send` — on a wsTransport page that seam is unwired today (same
    unwired-seam class as S24's resume), so an enter-key chat goes nowhere
    over WS. L1 asserts the exact §4.3 intent shape in the mock invocation
    log (`session_intent` stubbed `{ok:null}` under `tauri:false`), then
    drives the identical chat text through the real GM authority (liveHost
    stdin) and asserts the server-broadcast `chat_appended` renders exactly
    once — the WS decode/apply/reducer/render path is fully live end-to-end.
  - **L3 pins the real secret-roll semantics.** The authority DOES broadcast
    a placeholder entry for a secret roll — `(secret GM roll)` with
    `roll: null`, no expr, no total (phase10_table_demo.rs pins this
    server-side). L3's secrecy assertions are leak-shaped: no expr text, no
    `.roll-detail`, no `<strong>` totals anywhere in the player DOM, the
    placeholder is the only `data-kind="roll"` entry, and the public chat
    applied after it is the ordering witness (frames are strictly ordered on
    the one socket — once the public line renders, the secret patch was
    already applied above it).
  - **S27 stop 2 reuses the S14 adaptation.** The lobby invite panel
    (PIN/LAN/ticket verbatim) is unreachable after the host round-trip (12.2
    note); stop 2 drives the full host flow at 390 px — the real mobile host
    story — then asserts the true post-host lobby (host/join panels present,
    zero invite affordance).
  - **`HarnessOverrides` loosening, as 12.2's note pre-authorized.** Widened
    to `{ tauri?: boolean; [cmd: string]: Stub | boolean | undefined }` with
    one internal narrowing cast inside `installHarness`; resilience.spec.ts's
    S26-localized `as HarnessOverrides` cast and stale comment removed.
  - Live-lane knowledge for future maintainers: wsTransport pages never touch
    the mock registry — synchronize on DOM `expect` polls (15 s cap) and the
    liveHost `applied` ack only; the play page renders the chat input only
    under the ACTIVE Chat+Dice tab (a Party-tab → chat-fill ordering silently
    stalls to test timeout — L1 must click back before its chat leg);
    `e2e_host` is deterministic by construction (pinned session meta
    `lan-test`/`LAN Table`/`gm-peer`, `--pin` default `A7K9M2`, GM registered
    at boot like `service.rs host()`, adventure resolver serving only the
    embedded `tests/fixtures/adventure/widows-lamp.md`). Playwright 1.64
    quirk seen during development: a test that hits its timeout with a live
    WebSocket prints a secondary `apiRequestContext ENOENT …recording1.network`
    trace-copy error during teardown — teardown noise, not a second failure.
- **12.3 follow-ups landed** (2026-10-15, orchestrator-fixed post-12.3):
  +layout.svelte's `main` gets `box-sizing: border-box` (fixes the <792px
  32px overhang 12.3 caught) and `mobile.spec.ts` restores the strict
  `scrollWidth <= viewport` assertion (KNOWN_LAYOUT_OVERHANG_PX deleted);
  table lobby prefs race fixed — localStorage reads moved above the first
  `await` and the persist `$effect` is gated on `prefsLoaded` so its initial
  run can't clobber stored prefs with blank defaults (12.2's S14 evidence).
  Re-verified after fixes: `pnpm check` 0/0, `pnpm test` 86/86, `pnpm e2e`
  27/27 (strict assertion passing), `e2e:live` 4/4.
- **Post-12 backlog (not committed here):** runtime-injection probe against the
  production bundle (§2.2); visual baselines (`PF2E_VRT=1`, revisit per §5
  trigger); LLM token-stream scenario; nightly `e2e:live` workflow.

Guardrails restated for this phase: `cargo test` / `cargo test --all-features`
(0 fail, `PF2E_GUI_SMOKE`/`PF2E_NET_TESTS`/`PF2E_NET_WAN` still unset by
default), `pnpm test`, `pnpm check` = **0 errors / 0 warnings**, `pnpm build`
and `pnpm tauri build --no-bundle` untouched. Deterministic lane runs on every
PR with no network beyond the cached browser download, no display, no secrets
(LLM-off states are the *fixture*, not an env var), and no WAN.



