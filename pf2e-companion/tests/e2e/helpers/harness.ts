// Playwright-side helpers for the Phase 12 harness (PLAN §3.3).
//
// installHarness() seeds the in-page stub config before the app boots;
// the app bundle (harness mode) then serves every IPC call from it. The
// remaining helpers bridge the test script into window.__pf2eHarness.
import { expect, test, type Page } from "@playwright/test";
import type {
  Intent,
  LlmStatus,
  PatchEvent,
  PresenceEvent,
} from "$lib/ipc";
import { fixtureCampaign } from "../fixtures/vault";

/** LLM-off: the fixture state for every 12.x scenario (PLAN §1: LLM-off is the fixture). */
export const LLM_OFF: LlmStatus = {
  configured: false,
  provider: null,
  model: null,
  key_present: false,
};

/**
 * The baseline every route's boot needs (PLAN §3.3): the layout mounts
 * `list_lenses` + `list_campaigns`, the search page probes `schema_version`,
 * and LLM state defaults to off. The /table lobby refreshes its Saved
 * Sessions list on every mount (refreshSaved; the resume surface merged
 * post-12.3), so `session_list_saved` defaults to EMPTY here — S13–S15/S27
 * keep pinning "no saved panel" and only S24 overrides it.
 */
export const BASE_RESPONSES: Record<string, Stub> = {
  list_lenses: { ok: [] },
  list_campaigns: { ok: [fixtureCampaign] },
  schema_version: { ok: 12 },
  llm_status: { ok: LLM_OFF },
  session_list_saved: { ok: [] },
};

/**
 * Per-page harness config overrides. The index signature is widened with
 * `boolean` so the named escape hatch `tauri: false` type-checks (the 12.2
 * intersection `Record<string, Stub> & { tauri?: boolean }` made `tauri`
 * unsatisfiable; §9 12.2 note). Command values are still runtime-checked by
 * the mock dispatcher — a non-Stub fails fast inside the page.
 */
export type HarnessOverrides = {
  /** `false` leaves __TAURI_INTERNALS__ unset → createTransport picks wsTransport. */
  tauri?: boolean;
  [cmd: string]: Stub | boolean | undefined;
};

/**
 * Seed `window.__PF2E_HARNESS_CONFIG__` (BASE_RESPONSES ∪ overrides) before
 * the app's first script runs. `tauri: false` leaves __TAURI_INTERNALS__
 * unset so transport.ts picks wsTransport (live/WS lanes, PLAN §3.4).
 */
export async function installHarness(
  page: Page,
  overrides: HarnessOverrides = {},
): Promise<void> {
  const { tauri = true, ...responseOverrides } = overrides;
  // Narrowing cast: the widened index signature above is the call-site
  // contract; the page-side config stays strictly Record<string, Stub>.
  const responses = {
    ...BASE_RESPONSES,
    ...(responseOverrides as Record<string, Stub>),
  };
  await page.addInitScript(
    (cfg) => {
      window.__PF2E_HARNESS_CONFIG__ = cfg;
    },
    { responses, tauri },
  );
}

/** Push one PatchEvent into the page's "session:patch" listeners. */
export async function emitPatch(page: Page, patch: PatchEvent): Promise<void> {
  await page.evaluate(
    (p) => window.__pf2eHarness.emit("session:patch", p),
    patch,
  );
}

/** Push one PresenceEvent into the page's "session:presence" listeners. */
export async function emitPresence(
  page: Page,
  ev: PresenceEvent,
): Promise<void> {
  await page.evaluate(
    (p) => window.__pf2eHarness.emit("session:presence", p),
    ev,
  );
}

/**
 * Await at least one registered listener for `event` (PLAN §3.2 readiness
 * probe). Mandatory before the first emit of every gameflow spec: the play
 * page attaches listeners in onMount, and emitting earlier silently drops
 * patches.
 */
export async function waitForListener(
  page: Page,
  event: string,
): Promise<void> {
  await expect
    .poll(() => page.evaluate((e) => window.__pf2eHarness.listenerCount(e), event))
    .toBeGreaterThan(0);
}

/** The append-only mocked-IPC record, oldest first. */
export async function invocations(page: Page): Promise<InvocationRecord[]> {
  return page.evaluate(() => window.__pf2eHarness.invocations);
}

/**
 * Assert the page recorded at least one `session_intent` call whose intent
 * has `kind` and matches `matcher` (objectContaining). E.g.
 * `expectIntents(page, "roll_dice", { expr: "1d20+7" })`.
 *
 * Bounded poll, not a one-shot read: post-F-R7 the play page's sends ride
 * the transport promise chain (click → transport.send → dynamic import →
 * invoke), so the record lands a task-hop after the action resolves — the
 * same readiness-probe class as waitForListener (PLAN §3.2).
 */
export async function expectIntents(
  page: Page,
  kind: Intent["kind"],
  matcher: Record<string, unknown> = {},
): Promise<void> {
  const matchingIntents = async () =>
    (await invocations(page))
      .filter((r) => r.cmd === "session_intent")
      .map((r) => (r.args as { intent: Intent }).intent)
      .filter((i) => i.kind === kind);
  await expect
    .poll(async () => (await matchingIntents()).length, {
      message: `expected ≥1 session_intent of kind "${kind}"; got none`,
    })
    .toBeGreaterThan(0);
  expect(await matchingIntents()).toContainEqual(
    expect.objectContaining(matcher),
  );
}

/**
 * Evidence screenshot (PLAN §5): writes tests/e2e/screenshots/<relPath>.png
 * and attaches it to the HTML report. relPath starts with the spec stem
 * (e.g. "reference/04-entity-statblock"). Captured, never pixel-asserted.
 */
export async function shot(page: Page, relPath: string): Promise<void> {
  const path = `tests/e2e/screenshots/${relPath}.png`;
  await page.screenshot({ path, fullPage: true });
  await test.info().attach(`${relPath}.png`, { path, contentType: "image/png" });
}
