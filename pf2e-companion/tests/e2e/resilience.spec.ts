// Group F — resilience (PLAN §4, S25–S26).
//
// §4→reality adaptation (recorded in PLAN §9, 12.2 note): S26's planned
// "Connection lost" banner + Rejoin button are UNREACHABLE from a cold
// start in the deterministic lane. wsTransport rejects the initial
// connect() promise on the first refused socket (transport.ts onerror), the
// play page's init() catch sets the local error and schedules
// goto("/table") after 2 s — before the ~3.5 s retry budget
// (500/1000/2000 ms backoff, then `failed`) can elapse. The failed-banner +
// Rejoin surface only exists for an established-then-dropped connection,
// which needs a live server to establish — 12.3 live-lane territory. S26
// therefore pins the true cold-start failure surface: the redirecting error
// banner, then arrival back on the lobby.
import { expect, test } from "@playwright/test";
import {
  emitPatch,
  installHarness,
  shot,
  waitForListener,
} from "./helpers/harness";
import { joinViaLobby } from "./helpers/session";
import {
  fixtureP2pTicket,
  fixturePlayerName,
  fixtureSessionId,
  fixtureSessionInfoPlayer,
} from "./fixtures/session";

test("S25 error-banner", async ({ browser }) => {
  const context = await browser.newContext();
  const page = await context.newPage();
  try {
    await installHarness(page, {
      session_join_peer: { ok: fixtureSessionInfoPlayer },
    });
    await joinViaLobby(page, {
      name: fixturePlayerName,
      ticket: fixtureP2pTicket,
    });
    await waitForListener(page, "session:patch");

    // An authority rejection arrives as a session:error patch…
    await emitPatch(page, {
      session_id: fixtureSessionId,
      kind: "error",
      message: "authority: only the GM may reveal handouts",
    });
    const banner = page.locator(".session-error-banner");
    await expect(banner).toBeVisible();
    await expect(banner).toContainText(
      "authority: only the GM may reveal handouts",
    );
    await shot(page, "resilience/01-error-banner");

    // …and the ✕ dismiss clears it, back to a clean surface.
    await banner.getByRole("button", { name: "Dismiss" }).click();
    await expect(page.locator(".session-error-banner")).toHaveCount(0);
    await expect(page.getByRole("alert")).toHaveCount(0);
  } finally {
    await context.close();
  }
});

test("S26 ws-connection-failed", async ({ page }) => {
  // tauri:false — no __TAURI_INTERNALS__ flag, so createTransport() picks
  // the real (aliasing-free) wsTransport; the ?host= points at the discard
  // port (connection refused fast). IPC stubs still cover the layout boot.
  // (The 12.2 `as HarnessOverrides` workaround is gone — 12.3 loosened the
  // type properly, see PLAN §9.)
  await installHarness(page, { tauri: false });
  await page.goto("/table/play?name=Alice&host=ws://127.0.0.1:9/x");

  // True cold-start failure surface (see header note): the refused socket
  // rejects connect(), init() catches it, and the redirecting banner shows.
  const banner = page.locator(".error-banner");
  await expect(banner).toBeVisible();
  await expect(banner).toContainText("WebSocket connection failed");
  await expect(banner).toContainText("Redirecting to lobby");
  await shot(page, "resilience/02-connection-failed");

  // The redirect fires ~2 s after the failure — inside this expect window —
  // and the lobby comes up on its normal host/join panels.
  await expect(page).toHaveURL(/\/table$/);
  await expect(
    page.getByRole("heading", { level: 2, name: "Host a Session" }),
  ).toBeVisible();
});
