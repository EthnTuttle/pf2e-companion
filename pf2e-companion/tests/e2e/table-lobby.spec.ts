// Group D — table lobby (PLAN §4, S13–S15).
// Every IPC call the lobby route makes is stubbed via installHarness; each
// test gets a fresh Playwright context, so pf2e.playerName prefs start empty
// and identity is deterministic (PLAN §4 Group D note).
//
// §4→reality adaptations (recorded in PLAN §9, 12.2 note):
// - S14: the planned post-return invite panel (PIN/LAN/ticket verbatim +
//   "Enter Table →") is unreachable in the current app. The invite strings
//   live in lobby component-local state (myTicket/myLanUrl, set inside
//   handleHost, which immediately goto()s to /table/play), and returning to
//   the lobby runs the play page's onDestroy → resetSession(), which clears
//   the sessionState.info gate the invite panel also needs. Nothing
//   invite-related persists (localStorage holds only name/color prefs).
//   S14 therefore asserts the true observable contract: the full host flow,
//   its exact session_host args, the auto-navigation, the persisted GM
//   identity on the play surface, and — back in the lobby — that no
//   invite/resume affordance currently renders.
// - S15: the lobby's inline error surface is <p class="err"> — unlike the
//   play surface it has no role="alert". Assertions target .err.
import { expect, test } from "@playwright/test";
import { installHarness, invocations, shot } from "./helpers/harness";
import { hostViaLobby } from "./helpers/session";
import { fixtureSessionInfoGm, fixtureSessionName } from "./fixtures/session";

test("S13 lobby-no-campaign", async ({ page }) => {
  await installHarness(page, { list_campaigns: { ok: [] } });
  await page.goto("/table");

  await expect(
    page.getByRole("heading", { level: 1, name: "Live Table" }),
  ).toBeVisible();
  // Guidance copy with the Vault link…
  await expect(page.locator("p.muted")).toContainText(
    "Create a campaign in the",
  );
  await expect(page.locator('p.muted a[href="/vault"]')).toBeVisible();
  // …and the Host button stays disabled even with an identity filled in
  // (the gate is the missing campaign, not the name).
  await page.getByLabel("Name", { exact: true }).fill("GM Dana");
  await expect(
    page.getByRole("button", { name: "Host Session" }),
  ).toBeDisabled();
  await shot(page, "table-lobby/01-no-campaign");
});

test("S14 lobby-host-invite", async ({ page }) => {
  await installHarness(page, {
    session_host: { ok: fixtureSessionInfoGm },
  });
  await hostViaLobby(page, {
    name: "GM Dana",
    adventureSlug: "widows-lamp",
  });

  // Auto-navigation landed on the play surface; the GM identity persisted
  // across the client-side route change (badge + peer prefix render from
  // sessionState.info before any snapshot arrives).
  await expect(page.locator(".badge.gm")).toHaveText("GM");
  await expect(page.locator(".peer-id")).toHaveText("gm-peer-");

  // The exact session_host args the Rust authority layer expects.
  const hosts = (await invocations(page)).filter(
    (r) => r.cmd === "session_host",
  );
  expect(hosts).toHaveLength(1);
  expect(hosts[0]?.args).toEqual({
    campaignId: "millbrook",
    name: fixtureSessionName,
    adventureSlug: "widows-lamp",
  });

  // Back to the lobby the way a user goes: the bottom tab bar (client-side
  // nav — a full page.goto would re-seed stubs and clear the log anyway).
  await page.locator('.tabbar a[href="/table"]').click();
  await expect(
    page.getByRole("heading", { level: 2, name: "Host a Session" }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { level: 2, name: "Join a Session" }),
  ).toBeVisible();
  // CURRENT TRUTH (see header note): no invite panel survives the
  // host→play→lobby round-trip — the session state was reset by the play
  // page's onDestroy and the ticket/URL were lobby-component-local. When a
  // share/resume surface lands, restore §4's verbatim PIN/LAN/ticket + the
  // "Enter Table →" assertions here.
  await expect(page.locator(".invite")).toHaveCount(0);
  await shot(page, "table-lobby/02-invite");
});

test("S15 lobby-join-error", async ({ page }) => {
  await installHarness(page, {
    session_join_peer: { error: "no such ticket" },
  });
  await page.goto("/table");

  await page.getByLabel("Name", { exact: true }).fill("Alice");
  // The lobby defaults to the P2P Ticket join tab.
  await page.getByLabel("Ticket", { exact: true }).fill("bogus-ticket");
  await page.getByRole("button", { name: "Join Session" }).click();

  // Inline error (lobby surface: <p class="err">), no navigation, and the
  // recorded join attempt carried exactly the entered pair.
  await expect(page.locator("p.err")).toContainText("no such ticket");
  await expect(page).toHaveURL(/\/table$/);
  await shot(page, "table-lobby/03-join-error");
  const joins = (await invocations(page)).filter(
    (r) => r.cmd === "session_join_peer",
  );
  expect(joins).toHaveLength(1);
  expect(joins[0]?.args).toEqual({ ticket: "bogus-ticket", name: "Alice" });

  // The LAN tab state gets captured too (error message persists across the
  // tab switch — page-level state in the lobby).
  await page.getByRole("button", { name: "LAN URL", exact: true }).click();
  await expect(
    page.getByLabel("Host LAN URL", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByLabel("Table PIN (6 chars)", { exact: true }),
  ).toBeVisible();
  await shot(page, "table-lobby/04-join-lan-tab");
});
