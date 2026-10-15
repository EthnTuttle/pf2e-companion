// Shared table gameflow drivers (PLAN §3.3), built on helpers/harness.ts.
//
// hostViaLobby / joinViaLobby drive the real lobby UI — identity form, host
// form, ticket join — the same path a user takes. The lobby's handlers attach
// the (mocked) ipcTransport listeners for "session:patch"/"session:presence"
// and then auto-navigate to /table/play, so both helpers only return once
// the URL is the play surface AND both listeners are registered (the §3.3
// readiness rule; emitting earlier silently drops patches).
//
// emitSnapshotFromGm(gmPage, playerPage, …) takes the two pages as named
// parameters in GM→player order — a call site that swaps them reads wrong
// aloud (§8 risk 9).
import { expect, type Page } from "@playwright/test";
import { emitPatch, waitForListener } from "./harness";
import {
  fixtureSessionId,
  fixtureSessionName,
  gmSnapshotBuilder,
  type SnapshotOverrides,
} from "../fixtures/session";

/**
 * Host through the lobby and land on /table/play. Always hosts the fixture
 * session name (fixtureSessionName) so S14 can assert the exact recorded
 * session_host args; pass adventureSlug to pick the lobby's one hardcoded
 * adventure option.
 */
export async function hostViaLobby(
  page: Page,
  { name, adventureSlug }: { name: string; adventureSlug?: string },
): Promise<void> {
  await page.goto("/table");
  await page.getByLabel("Name", { exact: true }).fill(name);
  await page.getByLabel("Session Name").fill(fixtureSessionName);
  if (adventureSlug) {
    await page.getByLabel("Adventure (optional)").selectOption(adventureSlug);
  }
  await page.getByRole("button", { name: "Host Session" }).click();
  await expect(page).toHaveURL(/\/table\/play$/);
  await waitForListener(page, "session:patch");
  await waitForListener(page, "session:presence");
}

/**
 * Join through the lobby's P2P Ticket tab (the default join mode) and land
 * on /table/play. The ticket is the GM fixture's ticket — same as a player
 * pasting the string the GM copied out of the invite panel.
 */
export async function joinViaLobby(
  page: Page,
  { name, ticket }: { name: string; ticket: string },
): Promise<void> {
  await page.goto("/table");
  await page.getByLabel("Name", { exact: true }).fill(name);
  await page.getByLabel("Ticket", { exact: true }).fill(ticket);
  await page.getByRole("button", { name: "Join Session" }).click();
  await expect(page).toHaveURL(/\/table\/play$/);
  await waitForListener(page, "session:patch");
  await waitForListener(page, "session:presence");
}

/**
 * Resume a saved session from the lobby's Saved Sessions panel and land on
 * /table/play. The caller must already be on /table with the intended saved
 * session listed (session_list_saved stubbed to it); the identity name is
 * re-filled so the Resume button's `!playerName.trim()` gate is satisfied
 * even in a fresh context. Like the other lobby drivers, this only returns
 * once the URL is the play surface AND both listeners are registered (the
 * resume handler attaches them before goto, exactly like handleHost).
 */
export async function resumeViaLobby(
  page: Page,
  { name }: { name: string },
): Promise<void> {
  await page.getByLabel("Name", { exact: true }).fill(name);
  await page.getByRole("button", { name: "Resume" }).click();
  await expect(page).toHaveURL(/\/table\/play$/);
  await waitForListener(page, "session:patch");
  await waitForListener(page, "session:presence");
}

/**
 * Build ONE snapshot and deliver it as a "session:patch" to the GM page and
 * (when given) the player page — one builder call, so both contexts receive
 * byte-identical authority state. waitForListener polls precede each emit;
 * they are instantaneous after hostViaLobby/joinViaLobby but keep this safe
 * for any future caller that composes differently.
 */
export async function emitSnapshotFromGm(
  gmPage: Page,
  playerPage: Page | null,
  builderOpts: SnapshotOverrides = {},
): Promise<void> {
  const snapshot = gmSnapshotBuilder(builderOpts);
  await waitForListener(gmPage, "session:patch");
  await emitPatch(gmPage, {
    session_id: fixtureSessionId,
    kind: "snapshot",
    snapshot,
  });
  if (playerPage) {
    await waitForListener(playerPage, "session:patch");
    await emitPatch(playerPage, {
      session_id: fixtureSessionId,
      kind: "snapshot",
      snapshot,
    });
  }
}
