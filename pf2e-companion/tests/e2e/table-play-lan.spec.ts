// F-R7 deterministic lane — peer outbound intents over wsTransport (PLAN:
// docs/plans/PLAN-F-R7-peer-outbound.md; live counterpart: L6 in
// live-lan.spec.ts).
//
// Rig: the PLAYER page runs its REAL wsTransport against the MockWebSocket
// init-script (helpers/wsBridge.ts — the harness's init-script approach,
// tauri:false so createTransport picks ws); the GM page is the stock
// harness-IPC surface (hostViaLobby). The test script is the authority,
// same as Group E: it awaits the recorded client frame, then plays the
// host's broadcast back — emitPatch to the GM page, deliverWsFrame to the
// player — and asserts DOM on the receiving sides.
//
// Trust-model notes (phase10 PLAN §4.4; src-tauri/src/session/mod.rs
// authorize): a player page exposes NO state-intent controls (every GM-only
// action is isGm-gated, and ws-joined pages are structurally Player), while
// the REAL gate is the bridge's `svc.apply(peer, Role::Player, …)` —
// phase10_lan.rs pins that server-side. What this lane pins, per the F-R7
// seam analysis, is the frontend end of the trust story:
//   (1) outbound intents leave a ws-joined page as §4.3 "intent" frames
//       (the exact shape phase10_lan.rs:158 drives against the real bridge)
//       and NEVER through the session_intent IPC — the pre-fix dead path;
//   (2) the authority's rejection (HostFrame::Error to the sender) renders
//       in the session banner, so a rejected state intent is VISIBLE.
import { expect, test, type Browser, type Page } from "@playwright/test";
import {
  emitPatch,
  installHarness,
  invocations,
  shot,
} from "./helpers/harness";
import {
  awaitWsFrame,
  deliverWsFrame,
  expectWsIntent,
  installWsMock,
} from "./helpers/wsBridge";
import { hostViaLobby } from "./helpers/session";
import {
  chatEntry,
  fixtureGmName,
  fixturePlayerName,
  fixturePlayerPeerId,
  fixturePlayersGmAndAlice,
  fixtureSessionId,
  fixtureSessionInfoGm,
  fixtureSessionName,
  fixtureSessionPin,
  gmSnapshotBuilder,
} from "./fixtures/session";

/** The page's chat composer (rendered in the default Chat+Dice tab). */
const composer = (page: Page) =>
  page.getByPlaceholder("Type a message or /roll 1d20+7");

/**
 * Join a player over the mocked wsTransport and drive the host's
 * handshake + first snapshot (exactly the HostFrame serde shapes
 * src-tauri/src/lan.rs serializes: welcome, then the patch frame with the
 * event flattened beside `type`). Returns after the joined end state is
 * visible — same assertions as the live lane's joinPlayer.
 */
async function joinPlayerOverMockLan(
  page: Page,
  snapshot: ReturnType<typeof gmSnapshotBuilder> = gmSnapshotBuilder({
    players: fixturePlayersGmAndAlice,
  }),
): Promise<void> {
  await installHarness(page, { tauri: false });
  await installWsMock(page);
  const hostUrl = `ws://e2e.mock/${fixtureSessionId}`;
  await page.goto(
    `/table/play?name=${encodeURIComponent(fixturePlayerName)}` +
      `&host=${encodeURIComponent(hostUrl)}` +
      `&pin=${encodeURIComponent(fixtureSessionPin)}`,
  );
  // The page's wsTransport must open with the real §4.3 hello — recording
  // it IS the first assert (a stopped IPC page would send nothing).
  const hello = await awaitWsFrame(page, (f) => f.type === "hello", "hello");
  expect(hello).toMatchObject({
    session_id: fixtureSessionId,
    name: fixturePlayerName,
    pin: fixtureSessionPin,
  });
  await deliverWsFrame(page, {
    type: "welcome",
    session_id: fixtureSessionId,
    peer_id: fixturePlayerPeerId,
    role: "player",
  });
  await deliverWsFrame(page, {
    type: "patch",
    session_id: fixtureSessionId,
    kind: "snapshot",
    snapshot,
  });
  await expect(
    page.getByRole("heading", { level: 1, name: fixtureSessionName }),
  ).toBeVisible();
  await expect(page.locator(".badge")).toHaveText("Player");
  await expect(page.locator(".error-banner")).toHaveCount(0);
  await expect(page.locator(".session-error-banner")).toHaveCount(0);
}

/** F-R7's regression pin, named: no intent may leave a ws-joined page via
 *  the session_intent IPC (pre-fix that was EVERY send — the dead seam). */
async function expectNoIntentIpc(page: Page): Promise<void> {
  const sessionIntents = (await invocations(page)).filter(
    (r) => r.cmd === "session_intent",
  );
  expect(sessionIntents).toHaveLength(0);
}

test("player chat reaches the wire — and the GM page renders it", async ({
  browser,
}) => {
  const gmContext = await browser.newContext();
  const aliceContext = await browser.newContext();
  const gmPage = await gmContext.newPage();
  const alicePage = await aliceContext.newPage();
  try {
    // GM surface: the stock harness-IPC path, hosting the fixture session.
    await installHarness(gmPage, {
      session_host: { ok: fixtureSessionInfoGm },
      session_intent: { ok: null },
    });
    await hostViaLobby(gmPage, { name: fixtureGmName });
    // One builder call, both surfaces (the emitSnapshotFromGm rule): the GM
    // gets the patch over the IPC emitter, the player over the ws mock —
    // byte-identical authority state, different real transports.
    const snapshot = gmSnapshotBuilder({ players: fixturePlayersGmAndAlice });
    await emitPatch(gmPage, {
      session_id: fixtureSessionId,
      kind: "snapshot",
      snapshot,
    });
    await expect(
      gmPage.getByRole("heading", { level: 1, name: fixtureSessionName }),
    ).toBeVisible();

    await joinPlayerOverMockLan(alicePage, snapshot);

    // Player sends chat through the composer → exactly one §4.3 intent
    // frame on the wire, nothing on the IPC log, no optimistic echo.
    const text = "the widow's lamp flickers";
    await composer(alicePage).fill(text);
    await composer(alicePage).press("Enter");
    await expectWsIntent(alicePage, "chat", { text });
    await expectNoIntentIpc(alicePage);
    await expect(alicePage.locator(".chat-log .msg")).toHaveCount(0);

    // The authority relays the appended entry to both surfaces (the reverse
    // direction of the join fixtures: peer → host → broadcast).
    const entry = chatEntry({ from: fixturePlayerName, kind: "chat", text });
    await emitPatch(gmPage, {
      session_id: fixtureSessionId,
      kind: "chat_appended",
      entry,
    });
    await deliverWsFrame(alicePage, {
      type: "patch",
      session_id: fixtureSessionId,
      kind: "chat_appended",
      entry,
    });

    // GM DOM picks up the peer's line; the sender's DOM echoes the
    // broadcast (the full round trip that was dead on device, F-R7/A6).
    for (const p of [gmPage, alicePage]) {
      await expect(p.locator(".chat-log .msg")).toHaveCount(1);
      await expect(p.locator(".chat-log")).toContainText(text);
    }
    await shot(gmPage, "table-play-lan/01-gm-renders-peer-chat");
    await shot(alicePage, "table-play-lan/02-player-echo-after-broadcast");
  } finally {
    await gmContext.close();
    await aliceContext.close();
  }
});

test("player rolls ride the same ws seam (quick roll + /roll --secret)", async ({
  browser,
}) => {
  const context = await browser.newContext();
  const page = await context.newPage();
  try {
    await joinPlayerOverMockLan(page);

    const dice = page.locator('.dice-quick input[placeholder="1d20+7"]');
    await dice.fill("1d20+7");
    await dice.press("Enter");
    await expectWsIntent(page, "roll_dice", { expr: "1d20+7", secret: false });

    await composer(page).fill("/roll 2d6 --secret");
    await composer(page).press("Enter");
    await expectWsIntent(page, "roll_dice", { expr: "2d6", secret: true });

    await expectNoIntentIpc(page);
  } finally {
    await context.close();
  }
});

test("authority rejection (error frame) renders in the session banner", async ({
  browser,
}) => {
  const context = await browser.newContext();
  const page = await context.newPage();
  try {
    await joinPlayerOverMockLan(page);

    // HostFrame::Error to the sender only — serde shape per lan.rs. This is
    // what a player's rejected state intent looks like on the wire (the
    // bridge's authorize(Role::Player) gate; L6 drives the real rejection).
    await deliverWsFrame(page, {
      type: "error",
      message: "forbidden: this intent is reserved for the GM",
      rejected_intent: { kind: "set_initiative", order: ["sneaky"] },
    });
    const banner = page.locator(".session-error-banner");
    await expect(banner).toBeVisible();
    await expect(banner).toContainText("reserved for the GM");
    await banner.getByRole("button", { name: "Dismiss" }).click();
    await expect(banner).toHaveCount(0);
  } finally {
    await context.close();
  }
});
