// Live lane (PLAN §4, L1–L4; architecture §3.4) — one real Chromium page
// talking the §4.3 WS frame protocol to the REAL Rust `lan` server
// (src-tauri/examples/e2e_host.rs, spawned via server/liveHost.ts).
//
// Gating: every test is under `@live` and self-skips unless PF2E_E2E_LAN=1
// (pnpm e2e:live) — the same pattern as PF2E_NET_TESTS in phase10_p2p.rs.
// Not a PR gate in v1 (PLAN §5).
//
// Synchronization discipline (§8 risk 7, §9 12.2 note): wsTransport pages
// never touch the mock registry, so waitForListener/emitPatch do not apply.
// Everything awaits bounded `expect` polls on DOM state or the liveHost
// `applied` ack — no sleeps. L3 uses the public-entry-after-secret ordering
// witness: frames arrive strictly ordered on the one socket, so once the
// public line renders, the secret roll's patch was already applied.
//
// App-truth adaptations (full prose in PLAN §9, 12.3 note):
// - L1's "send a chat": the play page's outbound sessionIntent is the
//   ipc.ts invoke — on a wsTransport page that seam is UNWIRED today
//   (wsTransport.send has no caller, same seam class as S24's resume). L1
//   therefore asserts the outbound intent SHAPE via the mock invocation log
//   (`session_intent` stubbed {ok:null}), then drives the identical chat
//   through the real server as GM and asserts the broadcast renders — the
//   receive path is fully live end-to-end.
// - L3's "no secret-roll entry": the real authority DOES broadcast a
//   number-less placeholder entry ("(secret GM roll)", roll null —
//   phase10_table_demo.rs pins this server-side). The player-visible
//   secrecy guarantee is that expr+total never leak; L3 pins exactly that,
//   plus the placeholder as the only roll-kind entry.
// - L5 (F-R1): the lobby's Join button is the path BOTH real-device failures
//   took (browser typed entry, native deep-link prefill). No LAN-join IPC
//   exists — the lobby hands the credentials to /table/play as ?host=
//   params and createTransport picks wsTransport even with Tauri internals
//   present. L5 therefore runs with `tauri: true` (the shell class that
//   failed natively) and drives TWO entry flows through the one button.
import { expect, test, type Page } from "@playwright/test";
import { expectIntents, installHarness } from "./helpers/harness";
import {
  liveLaneEnabled,
  spawnLiveHost,
  type LiveHost,
} from "./server/liveHost";
import { fixtureGmName, fixturePlayerName } from "./fixtures/session";

/** DOM polls in the live lane get a wider (still bounded) window. */
const LIVE = { timeout: 15_000 };

test.describe("@live LAN loopback lane (PF2E_E2E_LAN=1)", () => {
  test.skip(
    !liveLaneEnabled(),
    "loopback live lane is env-gated — run with PF2E_E2E_LAN=1 (pnpm e2e:live)",
  );

  let host: LiveHost | undefined;
  test.beforeEach(async () => {
    host = await spawnLiveHost();
  });
  test.afterEach(async () => {
    await host?.close();
    host = undefined;
  });

  /**
   * Join a player page over the real wsTransport and await the full
   * welcome → snapshot arrival (h1 adopts the live session name; Player
   * badge; zero error banners). Stubs: base layout boot + the play page's
   * unwired outbound session_intent ({ok:null} records, never throws) —
   * `tauri:false` leaves __TAURI_INTERNALS__ unset (PLAN §3.4).
   */
  async function joinPlayer(page: Page) {
    await installHarness(page, {
      tauri: false,
      session_intent: { ok: null },
    });
    await page.goto(host!.playerUrl(fixturePlayerName));
    await expect(
      page.getByRole("heading", { level: 1, name: host!.sessionName }),
    ).toBeVisible(LIVE);
    await expect(page.locator(".badge")).toHaveText("Player", LIVE);
    await expect(page.locator(".badge.gm")).toHaveCount(0);
    await expect(page.locator(".error-banner")).toHaveCount(0);
    await expect(page.locator(".session-error-banner")).toHaveCount(0);
  }

  test("L1 join-snapshot-chat", async ({ page }) => {
    test.setTimeout(120_000);
    // joinPlayer awaits welcome (transport.connect resolved) + snapshot.
    await joinPlayer(page);

    // Party tab: the roster the real server broadcasts — the host's
    // boot-registered GM plus the just-joined player.
    await page
      .locator(".play-tabs")
      .getByRole("tab", { name: "Party", exact: true })
      .click();
    await expect(page.locator(".party-card")).toHaveCount(2, LIVE);
    await expect(page.locator(".party-panel")).toContainText(fixtureGmName);
    await expect(page.locator(".party-panel")).toContainText(fixturePlayerName);

    // Chat leg (see header note): back to Chat+Dice — the input renders only
    // in the active tab's panel. The page's submit produces the exact
    // §4.3 intent shape (mock invocation log), with no optimistic echo…
    await page
      .locator(".play-tabs")
      .getByRole("tab", { name: "Chat+Dice", exact: true })
      .click();
    const chatText = "hello over the live lane";
    const chatInput = page.getByPlaceholder("Type a message or /roll 1d20+7");
    await chatInput.fill(chatText);
    await chatInput.press("Enter");
    await expectIntents(page, "chat", { text: chatText });
    await expect(page.locator(".chat-log .msg")).toHaveCount(0);

    // …and the same chat applied by the real GM authority broadcasts back
    // over the wire and renders exactly once.
    const events = await host!.gmIntent({ kind: "chat", text: chatText });
    expect(events.filter((e) => e.kind === "chat_appended")).toHaveLength(1);
    await expect(page.locator(".chat-log .msg")).toHaveCount(1, LIVE);
    await expect(page.locator(".chat-log .msg .text")).toHaveText(chatText);
  });

  test("L2 wrong-pin-rejected", async ({ page }) => {
    test.setTimeout(120_000);
    await installHarness(page, { tauri: false });
    await page.goto(host!.playerUrl(fixturePlayerName, "ZZZZZZ"));

    // The server's pre-welcome error frame rejects transport.connect(); the
    // play page surfaces it and redirects to the lobby (~2 s). The welcome
    // never arrived — the h1 never adopts the session name.
    const banner = page.locator(".error-banner");
    await expect(banner).toBeVisible(LIVE);
    await expect(banner).toContainText("bad or missing table PIN");
    await expect(banner).toContainText("Redirecting to lobby");
    await expect(
      page.getByRole("heading", { level: 1, name: host!.sessionName }),
    ).toHaveCount(0);
    await expect(page).toHaveURL(/\/table$/, LIVE);
    await expect(
      page.getByRole("heading", { level: 2, name: "Host a Session" }),
    ).toBeVisible();
  });

  test("L3 secret-roll-filtering", async ({ page }) => {
    test.setTimeout(120_000);
    await joinPlayer(page);

    const events = await host!.gmIntent({
      kind: "roll_dice",
      expr: "2d6+3",
      secret: true,
    });
    // Server-truth check on the ack stream itself: one number-less
    // placeholder entry (phase10_table_demo.rs pins the same semantics).
    const appended = events.filter((e) => e.kind === "chat_appended");
    expect(appended).toHaveLength(1);
    expect(appended[0]?.entry?.text).toBe("(secret GM roll)");
    expect(appended[0]?.entry?.roll).toBeNull();

    // Ordering witness: the public chat applied AFTER the secret roll…
    const publicText = "The door clicks open.";
    await host!.gmIntent({ kind: "chat", text: publicText });
    // …renders — so the secret roll's patch is already applied above it.
    await expect(page.locator(".chat-log")).toContainText(publicText, LIVE);

    // Player-DOM secrecy: no expr, no totals, no roll breakdown anywhere.
    await expect(page.locator(".chat-log")).not.toContainText("2d6+3");
    await expect(page.locator(".chat-log .roll-detail")).toHaveCount(0);
    await expect(page.locator(".chat-log strong")).toHaveCount(0);
    // The placeholder is the one roll-kind entry, and it stays number-less.
    const rollMsgs = page.locator('.chat-log .msg[data-kind="roll"]');
    await expect(rollMsgs).toHaveCount(1);
    await expect(rollMsgs.first()).toContainText("(secret GM roll)");
  });

  test("L4 gm-load-adventure", async ({ page }) => {
    test.setTimeout(120_000);
    await joinPlayer(page);

    const events = await host!.gmIntent({
      kind: "load_adventure",
      slug: "widows-lamp",
    });
    expect(
      events.some((e) => e.kind === "adventure_loaded" && e.slug === "widows-lamp"),
    ).toBe(true);
    // The broadcast adventure_loaded patch lands on the player page.
    await expect(page.locator(".adventure-badge")).toContainText(
      "widows-lamp",
      LIVE,
    );
  });

  test("L5 join-via-lobby-button", async ({ page }) => {
    test.setTimeout(120_000);
    // F-R1 regression pin — REAL Join-button clicks against the REAL Rust
    // LAN server, with Tauri internals PRESENT (tauri: true): the exact
    // shell class in which the native deep-link join died on device, and
    // the case createTransport's host-param-first rule exists for. No
    // session_join_* IPC is stubbed or expected — there is none to call.
    await installHarness(page);
    const lanUrl = `ws://127.0.0.1:${host!.port}/${host!.sessionId}`;

    // ── Leg 1: typed entry (device lanes A/B8 typed equivalent) ──────────
    await page.goto("/table");
    await page.getByLabel("Name", { exact: true }).fill(fixturePlayerName);
    await page.getByRole("button", { name: "LAN URL", exact: true }).click();
    await page.getByLabel("Host LAN URL", { exact: true }).fill(lanUrl);
    await page
      .getByLabel("Table PIN (6 chars)", { exact: true })
      .fill(host!.pin.toLowerCase()); // lobby must normalize to uppercase
    await page.getByRole("button", { name: "Join Session" }).click();

    // Identical end state to the direct-URL path (L1's assertion set): the
    // h1 adopts the live session name when the snapshot patch lands.
    await expect(page).toHaveURL(/\/table\/play\?/);
    await expect(
      page.getByRole("heading", { level: 1, name: host!.sessionName }),
    ).toBeVisible(LIVE);
    await expect(page.locator(".badge")).toHaveText("Player", LIVE);
    await expect(page.locator(".badge.gm")).toHaveCount(0);
    await expect(page.locator(".error-banner")).toHaveCount(0);
    await expect(page.locator(".session-error-banner")).toHaveCount(0);

    // The roster broadcast is real: GM (boot-registered) + this join.
    await page
      .locator(".play-tabs")
      .getByRole("tab", { name: "Party", exact: true })
      .click();
    await expect(page.locator(".party-card")).toHaveCount(2, LIVE);
    await expect(page.locator(".party-panel")).toContainText(fixtureGmName);
    await expect(page.locator(".party-panel")).toContainText(fixturePlayerName);

    // Live GM→player data flow after the button-join (L1's receive half;
    // the page's outbound seam stays the known-unwired one — L1 note, F-R7).
    await page
      .locator(".play-tabs")
      .getByRole("tab", { name: "Chat+Dice", exact: true })
      .click();
    const chatText = "hello via the lobby button";
    await host!.gmIntent({ kind: "chat", text: chatText });
    await expect(page.locator(".chat-log .msg")).toHaveCount(1, LIVE);
    await expect(page.locator(".chat-log .msg .text")).toHaveText(chatText);

    // ── Leg 2: deep-link-prefilled lobby (device lane B4 → B8 tap) ───────
    // The pf2e://join handler navigates to /table?join=1&url=…&pin=…; the
    // lobby prefills and scrubs the URL. Join is the same manual button.
    await page.goto(
      `/table?join=1&url=${encodeURIComponent(lanUrl)}&pin=${encodeURIComponent(host!.pin)}`,
    );
    await expect(page.getByLabel("Host LAN URL", { exact: true })).toHaveValue(
      lanUrl,
    );
    await expect(
      page.getByLabel("Table PIN (6 chars)", { exact: true }),
    ).toHaveValue(host!.pin);
    await page.getByLabel("Name", { exact: true }).fill(fixturePlayerName);
    await page.getByRole("button", { name: "Join Session" }).click();

    // Same end state again; the same-name+PIN reattach keeps the roster
    // stable at GM + player (no duplicate card).
    await expect(
      page.getByRole("heading", { level: 1, name: host!.sessionName }),
    ).toBeVisible(LIVE);
    await expect(page.locator(".badge")).toHaveText("Player", LIVE);
    await page
      .locator(".play-tabs")
      .getByRole("tab", { name: "Party", exact: true })
      .click();
    await expect(page.locator(".party-card")).toHaveCount(2, LIVE);
  });
});
