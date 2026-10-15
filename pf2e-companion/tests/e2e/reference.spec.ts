// Group A — reference surfaces (PLAN §4, S01–S06).
// Every IPC call these routes make is stubbed via installHarness; assertions
// target the real rendered DOM of src/routes.
import { expect, test, type Page } from "@playwright/test";
import { installHarness, invocations, shot } from "./helpers/harness";
import {
  fixtureEntityWithStatblock,
  fixtureMissingEntityId,
  fixtureSearchHits,
} from "./fixtures/search";
import {
  fixtureAliasHit,
  fixtureCreatureXp,
  fixtureMiracleHit,
  fixtureXpExtreme,
  fixtureXpModerate,
} from "./fixtures/table";

/** Search/alias/miracle result rows, excluding the ".empty" hint row. */
const resultRows = (page: Page) => page.locator("ul.hits li:not(.empty)");

test("S01 search-results-filter", async ({ page }) => {
  await installHarness(page, { search: { ok: fixtureSearchHits } });
  await page.goto("/");

  await expect(page.getByText("schema v12")).toBeVisible();
  await expect(resultRows(page)).toHaveCount(3);
  for (const hit of fixtureSearchHits) {
    await expect(page.locator("ul.hits h2", { hasText: hit.title })).toBeVisible();
  }
  await shot(page, "reference/01-search-results");

  // Client-side type filter — no IPC involved, one row survives.
  await page.getByRole("button", { name: "Spells", exact: true }).click();
  await expect(resultRows(page)).toHaveCount(1);
  await expect(page.locator("ul.hits h2")).toHaveText("Force Barrage");
  await shot(page, "reference/02-search-type-filter");

  // Re-stub to an empty result set, re-query, and the empty state renders.
  await page.evaluate(() => {
    window.__PF2E_HARNESS_CONFIG__!.responses["search"] = { ok: [] };
  });
  await page.locator("input[type=search]").press("Enter");
  await expect(resultRows(page)).toHaveCount(0);
  await expect(page.locator("ul.hits li.empty")).toBeVisible();
  await shot(page, "reference/03-search-empty");
});

test("S02 entity-statblock", async ({ page }) => {
  await installHarness(page, {
    get_entity: {
      queue: [{ ok: fixtureEntityWithStatblock }, { ok: null }],
    },
  });

  // Risk-5 smoke: a direct deep link to a parameterized route must be served
  // (SPA fallback) and boot under `vite preview`. If this ever 404s, fall
  // back to entering via "/" and navigating client-side (see PLAN §8.5).
  await page.goto("/entity/lewisian%3Adeity-hosts");
  await expect(
    page.getByRole("heading", { level: 1, name: "YHWH of Hosts" }),
  ).toBeVisible();
  // PLAN asserts the statblock card via an "AC" label; the real component
  // (src/lib/Statblock.svelte, deity-shaped) has no AC row — assert the
  // card itself plus a label it actually renders.
  await expect(page.locator(".statblock")).toBeVisible();
  await expect(page.locator(".statblock .kind")).toHaveText("stat block");
  await expect(
    page.locator(".statblock .lbl", { hasText: "Divine attribute" }),
  ).toBeVisible();
  await shot(page, "reference/04-entity-statblock");

  // Client-side (same-document) navigation to the missing id — a full
  // page.goto would re-run the init script and re-seed the stub queue.
  await page.evaluate(
    (href) => {
      const a = document.createElement("a");
      a.href = href;
      document.body.appendChild(a);
      a.click();
      a.remove();
    },
    `/entity/${encodeURIComponent(fixtureMissingEntityId)}`,
  );
  await expect(page.locator(".err")).toContainText(
    `No entity with id "${fixtureMissingEntityId}"`,
  );
  await shot(page, "reference/05-entity-missing");

  const entityCalls = (await invocations(page)).filter(
    (r) => r.cmd === "get_entity",
  );
  expect(entityCalls.map((r) => (r.args as { id: string }).id)).toEqual([
    "lewisian:deity-hosts",
    "missing:id",
  ]);
});

test("S03 encounter-math", async ({ page }) => {
  await installHarness(page, {
    xp_budget: {
      queue: [{ ok: fixtureXpModerate }, { ok: fixtureXpExtreme }],
    },
    creature_xp: { ok: fixtureCreatureXp },
  });
  await page.goto("/encounter");

  await expect(page.locator(".result .num")).toHaveText("80");
  await shot(page, "reference/06-encounter-moderate");

  // Switching difficulty re-fires recompute() with the new difficulty arg.
  await page.getByRole("button", { name: "Extreme", exact: true }).click();
  await expect(page.locator(".result .num")).toHaveText("160");
  await shot(page, "reference/07-encounter-extreme");

  const budgets = (await invocations(page)).filter(
    (r) => r.cmd === "xp_budget",
  );
  expect(budgets).toHaveLength(2);
  expect(budgets[0]?.args).toEqual({ partySize: 4, difficulty: "moderate" });
  expect(budgets[1]?.args).toEqual({ partySize: 4, difficulty: "extreme" });
});

test("S04 aliases", async ({ page }) => {
  await installHarness(page, {
    lookup_alias: { queue: [{ ok: [fixtureAliasHit] }, { ok: [] }] },
  });
  await page.goto("/aliases");

  await expect(resultRows(page)).toHaveCount(1);
  await expect(page.locator(".hits .legacy")).toHaveText("Magic Missile");
  await expect(page.locator(".hits .remaster")).toHaveText("Force Barrage");
  await shot(page, "reference/08-aliases");

  // The queued empty result renders the empty state on re-query.
  await page.locator("input[type=search]").press("Enter");
  await expect(resultRows(page)).toHaveCount(0);
  await expect(page.locator("ul.hits li.empty")).toBeVisible();
});

test("S05 miracles", async ({ page }) => {
  await installHarness(page, {
    lookup_miracle: { ok: [fixtureMiracleHit] },
  });
  await page.goto("/miracles");

  await expect(resultRows(page)).toHaveCount(1);
  await expect(page.locator(".hits .spell .name")).toHaveText("Water Walk");
  await expect(page.locator(".hits .ref")).toHaveText("Mt 14:25");
  await expect(
    page.locator(".hits h2", { hasText: "Jesus walks on the sea" }),
  ).toBeVisible();
  await shot(page, "reference/09-miracles");
});

test("S06 about", async ({ page }) => {
  await installHarness(page);
  await page.goto("/about");

  await expect(
    page.getByRole("heading", { level: 1, name: "About this app" }),
  ).toBeVisible();
  // The licensing / citation-conventions copy.
  await expect(
    page.getByRole("heading", { name: "Where the content comes from" }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "License badges" }),
  ).toBeVisible();
  await expect(page.getByText("Open RPG Creative License")).toBeVisible();
  await shot(page, "reference/10-about");
});
