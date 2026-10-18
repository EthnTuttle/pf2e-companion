// Group M — mobile viewport (PLAN §4, S27). One 390×844 pass across five
// headline surfaces — home, lobby (post-host), play (player POV), vault,
// entity — asserting the two mobile invariants at every stop:
//   1. the bottom tab-bar (Search/Encounter/Vault/Aliases/Miracles/Table)
//      stays visible;
//   2. no horizontal overflow (strict: scrollWidth <= viewport — the 12.3-
//      found `main` box-sizing bug was fixed in the 12.3 follow-up).
//
// Selector notes for <480px (12.2 handoff, PLAN §9): the play-surface tab
// labels (.play-tabs .lbl) are display:none, which strips their accessible
// names — play tabs are selected by INDEX and confirmed via aria-selected,
// never by name. (The bottom app tab-bar keeps its labels, but is addressed
// by href anyway.)
//
// Stop 3 additionally pins the F1 fix (ui-review 2026-10-14): the chat
// composer's box must not reach the tab-bar's box at 390px.
//
// §4→reality adaptation (same seam as S14; PLAN §9 12.2 note): the lobby's
// post-host invite panel (PIN/LAN/ticket verbatim) is unreachable — invite
// strings live in lobby-local state wiped by navigation. Stop 2 drives the
// full host flow at 390px (the real mobile host story), then asserts the
// true post-host lobby: host/join panels, no invite affordance.
//
// Imagery seams (landed after 12.2): the EntityImage hero slot on entity
// detail and the cover/handout/scene-bg slots on the play page render
// nothing without promoted art — every fixture here carries no `image`
// keys, so the slots stay empty; each stop's overflow assertion is the
// regression tripwire for those seams.
import { expect, test, type Page } from "@playwright/test";
import { installHarness, shot } from "./helpers/harness";
import {
  emitSnapshotFromGm,
  hostViaLobby,
  joinViaLobby,
} from "./helpers/session";
import {
  fixtureEntityWithStatblock,
  fixtureSearchHits,
} from "./fixtures/search";
import { fixtureEntitySummaries } from "./fixtures/vault";
import {
  fixtureGmName,
  fixtureP2pTicket,
  fixturePlayerName,
  fixturePlayersGmAndAlice,
  fixtureSessionInfoGm,
  fixtureSessionInfoPlayer,
} from "./fixtures/session";

test.use({ viewport: { width: 390, height: 844 }, hasTouch: true });

/** §4's per-stop invariants: bottom tab-bar fully rendered + zero horizontal
 *  overflow at the 390px viewport (strict — the 12.3 `main` box-sizing fix
 *  landed in the 12.3 follow-up, so no chrome-overhang allowance remains). */
async function expectMobileChrome(page: Page, stop: string): Promise<void> {
  const tabbar = page.locator("nav.tabbar");
  await expect(tabbar, `${stop}: bottom tab-bar visible`).toBeVisible();
  for (const href of [
    "/",
    "/encounter",
    "/vault",
    "/aliases",
    "/miracles",
    "/table",
  ]) {
    await expect(
      tabbar.locator(`a[href="${href}"]`),
      `${stop}: tab-bar link ${href}`,
    ).toBeVisible();
  }
  const scrollWidth = await page.evaluate(
    () => document.documentElement.scrollWidth,
  );
  expect(
    scrollWidth,
    `${stop}: horizontal overflow (scrollWidth ${scrollWidth} > 390)`,
  ).toBeLessThanOrEqual(390);
}

test("S27 mobile-pass", async ({ page }) => {
  // One install for all five stops (queue stubs would re-seed on the full
  // navigations anyway — every stub here is idempotent).
  await installHarness(page, {
    // Stop 1 — S01 stubs.
    search: { ok: fixtureSearchHits },
    // Stop 2 — S14 stubs (host), Stop 3 — join as the player fixture.
    session_host: { ok: fixtureSessionInfoGm },
    session_join_peer: { ok: fixtureSessionInfoPlayer },
    session_intent: { ok: null },
    // Stop 4 — S07 stubs. Stop 5 — S02 stubs.
    list_entities: { ok: fixtureEntitySummaries },
    get_entity: { ok: fixtureEntityWithStatblock },
  });

  // ── Stop 1: / with S01 stubs ──────────────────────────────────────────
  await page.goto("/");
  await expect(page.getByText("schema v12")).toBeVisible();
  await expect(page.locator("ul.hits li:not(.empty)")).toHaveCount(3);
  await expectMobileChrome(page, "stop 1 home");
  await shot(page, "mobile/01-home");

  // ── Stop 2: /table lobby with S14 stubs, post-host ────────────────────
  await hostViaLobby(page, {
    name: fixtureGmName,
    adventureSlug: "widows-lamp",
  });
  // The full host form worked at 390px: we're on the play surface as GM.
  await expect(page.locator(".badge.gm")).toHaveText("GM");
  // Back to the lobby the way a user goes — the bottom tab bar (client-side
  // nav; a full goto would re-seed stubs and clear the log).
  await page.locator('nav.tabbar a[href="/table"]').click();
  await expect(
    page.getByRole("heading", { level: 2, name: "Host a Session" }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { level: 2, name: "Join a Session" }),
  ).toBeVisible();
  // CURRENT TRUTH (header note): no invite/resume affordance renders after
  // the host→play→lobby round-trip — restore §4's PIN assertion here when a
  // share surface lands.
  await expect(page.locator(".invite")).toHaveCount(0);
  // F-R6 (ui-review 2026-10-18): the Join submit must sit fully above the
  // fixed tab-bar at rest (scroll 0) — mirror of stop 3's F1 composer check
  // (1px tolerance). Regression guard for the native-lobby occlusion: the
  // button's natural spot (doc y≈963–1003 at 390×867) is under the tabbar's
  // top edge (814.8) until the pinned-CTA rule lifts it.
  {
    await page.evaluate(() => window.scrollTo(0, 0));
    const joinBox = await page
      .locator('.join-form button[type="submit"]')
      .boundingBox();
    const tabbarBox = await page.locator("nav.tabbar").boundingBox();
    if (!joinBox) throw new Error("stop 2 lobby: join button not laid out");
    if (!tabbarBox) throw new Error("stop 2 lobby: tab-bar not laid out");
    expect(
      joinBox.y + joinBox.height,
      `stop 2 lobby: join button bottom (${joinBox.y + joinBox.height}) ` +
        `above tab-bar top (${tabbarBox.y}) — F-R6 overlap`,
    ).toBeLessThanOrEqual(tabbarBox.y + 1);
  }
  await expectMobileChrome(page, "stop 2 lobby");
  await shot(page, "mobile/02-lobby-invite");

  // ── Stop 3: /table/play player POV (S16/S17-style) ────────────────────
  await joinViaLobby(page, {
    name: fixturePlayerName,
    ticket: fixtureP2pTicket,
  });
  // The helpers already returned with patch/presence listeners registered;
  // emit the player-joined snapshot straight into this page.
  await emitSnapshotFromGm(page, null, {
    players: fixturePlayersGmAndAlice,
  });

  await expect(page.locator(".badge")).toHaveText("Player");
  await expect(page.locator(".badge.gm")).toHaveCount(0);
  const playTabs = page.locator('.play-tabs [role="tab"]');
  await expect(playTabs).toHaveCount(5); // no GM tab for a player
  // Default tab is Chat+Dice — by index: chat is first (labels are
  // display:none at <480px, so never select by name).
  await expect(playTabs.nth(0)).toHaveAttribute("aria-selected", "true");
  await expect(page.locator(".chat-log p.empty")).toContainText(
    "No messages yet",
  );
  // F1 (ui-review 2026-10-14): the chat composer must sit fully above the
  // fixed bottom tab-bar at 390px — its bottom edge must not reach the
  // tab-bar's top edge (1px tolerance). Regression guard for .play-surface's
  // old `height: 100vh`, which parked the composer behind/below the tabbar
  // at scroll 0 (composer top 876px vs 844px viewport, mobile).
  {
    const composerBox = await page.locator(".chat-input-row").boundingBox();
    const tabbarBox = await page.locator("nav.tabbar").boundingBox();
    if (!composerBox) throw new Error("stop 3 play: composer not laid out");
    if (!tabbarBox) throw new Error("stop 3 play: tab-bar not laid out");
    expect(
      composerBox.y + composerBox.height,
      `stop 3 play: composer bottom (${composerBox.y + composerBox.height}) ` +
        `above tab-bar top (${tabbarBox.y}) — F1 overlap`,
    ).toBeLessThanOrEqual(tabbarBox.y + 1);
  }
  // Party is the third tab (chat, initiative, party, scene, handouts).
  await playTabs.nth(2).click();
  await expect(playTabs.nth(2)).toHaveAttribute("aria-selected", "true");
  await expect(page.locator(".party-card")).toHaveCount(2);
  await expect(page.locator(".party-card .card-header .name")).toHaveText([
    fixtureGmName,
    fixturePlayerName,
  ]);
  await expectMobileChrome(page, "stop 3 play");
  await shot(page, "mobile/03-play-player");

  // ── Stop 4: /vault with S07 stubs ─────────────────────────────────────
  await page.goto("/vault");
  await expect(
    page.getByRole("heading", { level: 1, name: "Vault" }),
  ).toBeVisible();
  await expect(page.locator(".entities .hits li:not(.empty)")).toHaveCount(
    fixtureEntitySummaries.length,
  );
  await expectMobileChrome(page, "stop 4 vault");
  await shot(page, "mobile/04-vault");

  // ── Stop 5: /entity/<id> with S02 stubs ───────────────────────────────
  await page.goto("/entity/lewisian%3Adeity-hosts");
  await expect(
    page.getByRole("heading", { level: 1, name: "YHWH of Hosts" }),
  ).toBeVisible();
  await expect(page.locator(".statblock")).toBeVisible();
  await expectMobileChrome(page, "stop 5 entity");
  await shot(page, "mobile/05-entity");
});
