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
// - L1's chat leg: pre-F-R7 this was a mock invocation-log shape assertion,
//   because the play page's outbound seam was the ipc.ts invoke — UNWIRED on
//   a wsTransport page (wsTransport.send had no caller; the documented F-R7
//   dead seam). Post-F-R7 every send rides the real transport, so the
//   composer's Enter round-trips peer → bridge → authority → broadcast →
//   this same DOM; L1 asserts that echo, and L6 pins the GM-side pickup via
//   the e2e_host bcast tap plus the §4.4 role gate.
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
import { installHarness, shot } from "./helpers/harness";
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
   * badge; zero error banners). Stubs: base layout boot only —
   * `tauri:false` leaves __TAURI_INTERNALS__ unset (PLAN §3.4), and
   * post-F-R7 there is deliberately NO session_intent stub: a regression
   * that sends over IPC on a ws-joined page now fails loudly ("harness:
   * no stub for …") instead of being silently absorbed.
   */
  async function joinPlayer(page: Page) {
    await installHarness(page, { tauri: false });
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

    // Chat leg, post-F-R7 (see header note): back to Chat+Dice — the input
    // renders only in the active tab's panel. The composer's Enter now rides
    // the real wsTransport: the bridge applies this PEER chat and the
    // broadcast echoes it back onto this same DOM. No optimistic echo is
    // involved — what renders is the server's round trip.
    await page
      .locator(".play-tabs")
      .getByRole("tab", { name: "Chat+Dice", exact: true })
      .click();
    const chatText = "hello over the live lane";
    const chatInput = page.getByPlaceholder("Type a message or /roll 1d20+7");
    await chatInput.fill(chatText);
    await chatInput.press("Enter");
    await expect(page.locator(".chat-log .msg")).toHaveCount(1, LIVE);
    await expect(page.locator(".chat-log .msg .text").first()).toHaveText(chatText);

    // GM→player leg unchanged: a GM-applied chat renders exactly once more.
    const events = await host!.gmIntent({ kind: "chat", text: chatText });
    expect(events.filter((e) => e.kind === "chat_appended")).toHaveLength(1);
    await expect(page.locator(".chat-log .msg")).toHaveCount(2, LIVE);
    await expect(page.locator(".chat-log .msg .text").nth(1)).toHaveText(chatText);
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
    // the outbound half of this same seam is L6's job — F-R7, now pinned).
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

  test("L6 peer-outbound-chat", async ({ page }) => {
    test.setTimeout(120_000);
    await joinPlayer(page);

    // (a) Real composer send: the player's chat rides the real wsTransport
    // as a §4.3 intent frame; the bridge applies it as Role::Player and the
    // broadcast tap proves the GM side picked it up (the tap's matcher is
    // armed BEFORE the send — stdout lines are only matched while awaited).
    const chatInput = page.getByPlaceholder("Type a message or /roll 1d20+7");
    const chatText = "peer outbound on the wire";
    const appliedPromise = host!.waitForBroadcast(
      (e) => e.kind === "chat_appended" && e.entry?.text === chatText,
      "the peer's chat_appended on the GM-side broadcast tap",
    );
    await chatInput.fill(chatText);
    await chatInput.press("Enter");
    const applied = await appliedPromise;
    expect(applied.entry?.kind).toBe("chat");

    // …and the same broadcast echoes the line onto the sender's own DOM —
    // the round trip that was a dead end on device (ui-reviews A6/F-R7).
    await expect(page.locator(".chat-log .msg")).toHaveCount(1, LIVE);
    await expect(page.locator(".chat-log .msg .text")).toHaveText(chatText);
    await shot(page, "live-lan/06-peer-outbound-chat");

    // (b) Role gate on the same real bridge (§4.4 trust model): a state
    // intent from a LAN peer is refused by authorize(Role::Player). The
    // page itself exposes NO player-reachable state-intent control (all
    // isGm-gated), so this leg drives the wire straight from the page — a
    // second real browser WebSocket against the real host, mirroring
    // phase10_lan.rs wrong_authority_intent_gets_error_and_state_unchanged.
    const rejection = await page.evaluate(
      async ({ port, sessionId, pin }) => {
        return await new Promise<{
          message: string;
          rejectedKind: string | null;
        } | null>((resolve) => {
          const ws = new WebSocket(`ws://127.0.0.1:${port}/ws/${sessionId}`);
          const timer = setTimeout(() => resolve(null), 10_000);
          ws.onopen = () => {
            ws.send(
              JSON.stringify({
                type: "hello",
                session_id: sessionId,
                name: "Rebel",
                pin,
              }),
            );
          };
          ws.onmessage = (ev) => {
            const frame = JSON.parse(String(ev.data)) as {
              type: string;
              message?: string;
              rejected_intent?: { kind?: string };
            };
            if (frame.type === "welcome") {
              ws.send(
                JSON.stringify({
                  type: "intent",
                  intent: { kind: "set_initiative", order: ["sneaky"] },
                }),
              );
            }
            if (frame.type === "error") {
              clearTimeout(timer);
              ws.close();
              resolve({
                message: String(frame.message),
                rejectedKind: frame.rejected_intent?.kind ?? null,
              });
            }
          };
          ws.onerror = () => {
            clearTimeout(timer);
            resolve(null);
          };
        });
      },
      { port: host!.port, sessionId: host!.sessionId, pin: host!.pin },
    );
    expect(
      rejection,
      "the bridge must answer a peer's set_initiative with an error frame",
    ).not.toBeNull();
    expect(rejection!.message).toContain("GM");
    expect(rejection!.rejectedKind).toBe("set_initiative");
    // The rejection went to the SENDER only: the joined player's DOM is
    // untouched (still exactly the one echoed chat line from leg (a)).
    await expect(page.locator(".chat-log .msg")).toHaveCount(1);
    await expect(page.locator(".session-error-banner")).toHaveCount(0);
  });
});
