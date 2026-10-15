// Group E — two-context table gameflow (PLAN §4, S16–S24; architecture §3.4).
// Two browser contexts per scenario — gmPage + alicePage, both
// harness-stubbed; the test script is the authority: it decides which
// patches each side receives, asserts intents on the SENDING side, and
// asserts DOM on the RECEIVING side. waitForListener(page, "session:patch")
// (and "session:presence") precedes the first emit of every test (§8 risk
// 2; the lobby drivers in helpers/session.ts already await the same probe —
// the inline calls keep each test's readiness locally grep-verifiable).
//
// §4→reality adaptations (full prose in PLAN §9, 12.2 note):
// - S16 "players: GM only": the real host roster always includes the GM
//   (service.rs host() peer_joined()s the GM at session start), while the
//   Party empty-hint renders only when the players map is EMPTY. S16 covers
//   both render states in sequence: empty roster → "No players yet" hint;
//   then the realistic GM-only roster → GM party card, hint gone.
// - S20 "GM drawer dice": the floating overlay drawer (showGmDrawer) has no
//   opener anywhere in the current markup; the GM tab's Dice sub-tab and its
//   "Roll (Secret)" submit are the reachable secret-roll surface — S20
//   drives that.
// - S23: after session_ended the reducer clears info+snapshot
//   (session.svelte.ts), so badges revert to "Player", the title falls back
//   to "Live Table", and the GM tab disappears on BOTH pages — that drained
//   surface is the assertion target.
// - S22: the UI holds no optimistic initiative order, so a bare "add Alice,
//   add GM" emits two single-entry set_initiative intents (each computed
//   from the unchanged snapshot). S22 emits the server-style initiative
//   echo patch after each Add — the same loop a live server creates — which
//   makes the second set_initiative cumulative, per §4's assertion.
// - S24: the resume UI has LANDED (fork merge — Saved Sessions lobby panel,
//   session_list_saved / session_load, handleResume at table/+page.svelte),
//   so the old resume-gap pin was replaced by the real resume flow per §4's
//   own supersede clause: saved-list stub → Resume click → exact
//   session_load args → /table/play → re-emitted snapshot repopulates.
import { expect, test, type Browser, type Page } from "@playwright/test";
import {
  emitPatch,
  emitPresence,
  expectIntents,
  installHarness,
  invocations,
  shot,
  waitForListener,
} from "./helpers/harness";
import {
  emitSnapshotFromGm,
  hostViaLobby,
  joinViaLobby,
  resumeViaLobby,
} from "./helpers/session";
import {
  chatEntry,
  fixtureGmName,
  fixtureGmPeerId,
  fixtureHandoutsGmOnly,
  fixtureP2pTicket,
  fixturePlayerName,
  fixturePlayerPeerId,
  fixturePlayersGmAndAlice,
  fixturePlayersGmOnly,
  fixtureSavedSession,
  fixtureScene,
  fixtureSessionId,
  fixtureSessionInfoGm,
  fixtureSessionInfoPlayer,
  fixtureSessionName,
} from "./fixtures/session";
import type { Intent, PatchEvent } from "$lib/ipc";

/** A tab button in the play-surface tab bar (icons are aria-hidden, so the
 *  accessible name is exactly the label). */
const tabByName = (page: Page, name: string) =>
  page.locator(".play-tabs").getByRole("tab", { name, exact: true });

const switchTab = (page: Page, name: string) => tabByName(page, name).click();

/** Emit one patch GM-first, then player — argument order names the roles so
 *  a swapped call site reads wrong aloud (§8 risk 9). */
async function emitToBoth(
  gmPage: Page,
  alicePage: Page,
  patch: PatchEvent,
): Promise<void> {
  await emitPatch(gmPage, patch);
  await emitPatch(alicePage, patch);
}

interface HostedPair {
  gmPage: Page;
  alicePage: Page;
  close: () => Promise<void>;
}

/** Per-test two-context rig (§3.4): GM hosts via the lobby, Alice joins via
 *  the GM's fixture ticket. Each context gets its own page + harness stubs. */
async function hostPair(browser: Browser): Promise<HostedPair> {
  const gmContext = await browser.newContext();
  const aliceContext = await browser.newContext();
  const close = async () => {
    await gmContext.close();
    await aliceContext.close();
  };
  const gmPage = await gmContext.newPage();
  const alicePage = await aliceContext.newPage();
  await installHarness(gmPage, {
    session_host: { ok: fixtureSessionInfoGm },
    session_intent: { ok: null },
  });
  await installHarness(alicePage, {
    session_join_peer: { ok: fixtureSessionInfoPlayer },
    session_intent: { ok: null },
  });
  await hostViaLobby(gmPage, { name: fixtureGmName });
  await joinViaLobby(alicePage, {
    name: fixturePlayerName,
    ticket: fixtureP2pTicket,
  });
  return { gmPage, alicePage, close };
}

test("S16 gm-empty-table", async ({ browser }) => {
  const gmContext = await browser.newContext();
  const gmPage = await gmContext.newPage();
  try {
    await installHarness(gmPage, {
      session_host: { ok: fixtureSessionInfoGm },
      session_intent: { ok: null },
    });
    await hostViaLobby(gmPage, { name: fixtureGmName });
    await waitForListener(gmPage, "session:patch");
    await waitForListener(gmPage, "session:presence");

    // Fresh table: empty roster first (see header note on the roster/hint
    // divergence) — chat and party empty states are the evidence.
    await emitSnapshotFromGm(gmPage, null, { players: {} });

    await expect(
      gmPage.getByRole("heading", { level: 1, name: fixtureSessionName }),
    ).toBeVisible();
    await expect(gmPage.locator(".badge.gm")).toHaveText("GM");
    await expect(gmPage.locator('.play-tabs [role="tab"]')).toHaveCount(6);
    for (const label of [
      "Chat+Dice",
      "Initiative",
      "Party",
      "Scene",
      "Handouts",
      "GM",
    ]) {
      await expect(tabByName(gmPage, label)).toBeVisible();
    }
    await expect(gmPage.locator(".chat-log p.empty")).toContainText(
      "No messages yet",
    );

    // Party tab: empty roster → the share-invite hint (and no cards).
    await switchTab(gmPage, "Party");
    await expect(gmPage.locator(".party-panel p.empty")).toContainText(
      "No players yet",
    );
    await expect(gmPage.locator(".party-card")).toHaveCount(0);
    await shot(gmPage, "gameflow/01-gm-empty-table");

    // …then the roster the real host actually broadcasts: the hint swaps
    // out for the GM's own party card.
    await emitSnapshotFromGm(gmPage, null, { players: fixturePlayersGmOnly });
    await expect(gmPage.locator(".party-card")).toHaveCount(1);
    await expect(
      gmPage.locator(".party-card .card-header .name"),
    ).toHaveText(fixtureGmName);
    await expect(gmPage.locator(".party-panel p.empty")).toHaveCount(0);
  } finally {
    await gmContext.close();
  }
});

test("S17 player-joins", async ({ browser }) => {
  const { gmPage, alicePage, close } = await hostPair(browser);
  try {
    // Positive-path join args (S15 covers the error path).
    const joins = (await invocations(alicePage)).filter(
      (r) => r.cmd === "session_join_peer",
    );
    expect(joins).toHaveLength(1);
    expect(joins[0]?.args).toEqual({
      ticket: fixtureP2pTicket,
      name: fixturePlayerName,
    });

    await waitForListener(gmPage, "session:patch");
    await waitForListener(gmPage, "session:presence");
    await waitForListener(alicePage, "session:patch");
    await waitForListener(alicePage, "session:presence");

    // Presence first, then the full GM+Alice roster snapshot to both pages.
    await emitPresence(gmPage, {
      session_id: fixtureSessionId,
      kind: "peer_joined",
      peer_id: fixturePlayerPeerId,
      name: fixturePlayerName,
      via: "lan",
    });
    await emitPresence(alicePage, {
      session_id: fixtureSessionId,
      kind: "peer_joined",
      peer_id: fixturePlayerPeerId,
      name: fixturePlayerName,
      via: "lan",
    });
    await emitSnapshotFromGm(gmPage, alicePage, {
      players: fixturePlayersGmAndAlice,
    });

    // GM POV — Party tab shows Alice connected (not dimmed) next to the GM.
    await switchTab(gmPage, "Party");
    await expect(gmPage.locator(".party-card")).toHaveCount(2);
    const aliceCardOnGm = gmPage
      .locator(".party-card")
      .filter({ hasText: fixturePlayerName });
    await expect(aliceCardOnGm).toHaveCount(1);
    await expect(aliceCardOnGm.locator(".card-header .name")).toHaveText(
      fixturePlayerName,
    );
    await expect(aliceCardOnGm.locator(".card-header .peer")).toHaveText(
      fixturePlayerPeerId.slice(0, 8),
    );
    await expect(aliceCardOnGm).not.toHaveClass(/dimmed/);
    await shot(gmPage, "gameflow/02-player-joined-gm-pov");

    // Player POV — Player badge, no GM tab, party list has both names.
    await expect(alicePage.locator(".badge")).toHaveText("Player");
    await expect(alicePage.locator(".badge.gm")).toHaveCount(0);
    await expect(alicePage.locator('.play-tabs [role="tab"]')).toHaveCount(5);
    await expect(tabByName(alicePage, "GM")).toHaveCount(0);
    await switchTab(alicePage, "Party");
    const cards = alicePage.locator(".party-card");
    await expect(cards).toHaveCount(2);
    await expect(alicePage.locator(".party-card .card-header .name")).toHaveText(
      [fixtureGmName, fixturePlayerName],
    );
    await shot(alicePage, "gameflow/03-player-joined-player-pov");
  } finally {
    await close();
  }
});

test("S18 load-adventure", async ({ browser }) => {
  const { gmPage, alicePage, close } = await hostPair(browser);
  try {
    await waitForListener(gmPage, "session:patch");
    await waitForListener(gmPage, "session:presence");
    await waitForListener(alicePage, "session:patch");
    await waitForListener(alicePage, "session:presence");
    await emitSnapshotFromGm(gmPage, alicePage, {
      players: fixturePlayersGmAndAlice,
    });

    // GM tab → "Load Widow's Lamp" → the exact load_adventure intent.
    await switchTab(gmPage, "GM");
    await gmPage
      .getByRole("button", { name: "Load Widow's Lamp" })
      .click();
    await expectIntents(gmPage, "load_adventure", { slug: "widows-lamp" });

    // The authority broadcasts adventure_loaded: header badge on both pages.
    await emitToBoth(gmPage, alicePage, {
      session_id: fixtureSessionId,
      kind: "adventure_loaded",
      slug: "widows-lamp",
    });
    await expect(gmPage.locator(".adventure-badge")).toHaveText(
      "📖 widows-lamp",
    );
    await expect(alicePage.locator(".adventure-badge")).toHaveText(
      "📖 widows-lamp",
    );

    // scene_set renders the fixture grid + token on both pages.
    await emitToBoth(gmPage, alicePage, {
      session_id: fixtureSessionId,
      kind: "scene_set",
      scene: fixtureScene,
    });
    await switchTab(gmPage, "Scene");
    await switchTab(alicePage, "Scene");
    await expect(gmPage.locator(".token")).toHaveCount(1);
    await expect(alicePage.locator(".token")).toHaveCount(1);
    await expect(gmPage.locator(".token")).toHaveAttribute(
      "aria-label",
      "Token Widow at 3,2",
    );
    await expect(gmPage.locator(".scene-legend")).toContainText(
      "Grid: 10×8 (32px)",
    );

    // The follow-up snapshot carries the GM-only handout (snapshot replaces
    // state wholesale, so it repeats adventure + scene).
    await emitSnapshotFromGm(gmPage, alicePage, {
      players: fixturePlayersGmAndAlice,
      adventureSlug: "widows-lamp",
      scene: fixtureScene,
      handouts: fixtureHandoutsGmOnly,
    });

    // GM Handouts tab: the hidden handout lists, with its reveal affordance.
    await switchTab(gmPage, "Handouts");
    await expect(gmPage.locator(".handout-card h4")).toHaveText("letter");
    await expect(
      gmPage.getByRole("button", { name: "Reveal to Players" }),
    ).toBeVisible();
    await shot(gmPage, "gameflow/04-adventure-loaded-gm");

    // Player POV: same adventure badge + scene…
    await expect(alicePage.locator(".adventure-badge")).toHaveText(
      "📖 widows-lamp",
    );
    await shot(alicePage, "gameflow/05-adventure-loaded-player");
    // …but the GM-only handout is invisible: the tab shows the empty state.
    await switchTab(alicePage, "Handouts");
    await expect(alicePage.locator(".handout-card")).toHaveCount(0);
    await expect(alicePage.locator(".handouts-panel p.empty")).toContainText(
      "No handouts revealed yet",
    );
    await shot(alicePage, "gameflow/06-handouts-hidden-player");
  } finally {
    await close();
  }
});

test("S19 first-roll", async ({ browser }) => {
  const { gmPage, alicePage, close } = await hostPair(browser);
  try {
    await waitForListener(gmPage, "session:patch");
    await waitForListener(gmPage, "session:presence");
    await waitForListener(alicePage, "session:patch");
    await waitForListener(alicePage, "session:presence");
    await emitSnapshotFromGm(gmPage, alicePage, {
      players: fixturePlayersGmAndAlice,
    });

    // Player quick-rolls from the Chat+Dice tab (the default tab): dice
    // input, Enter. (The check on .dice-quick input[placeholder] matters:
    // .dice-quick also holds the Secret checkbox input.)
    await alicePage.locator('.dice-quick input[placeholder="1d20+7"]').fill("1d20+7");
    await alicePage.locator('.dice-quick input[placeholder="1d20+7"]').press("Enter");

    // Sending side: the exact intent, secret falsy. No optimistic local
    // echo — the chat log stays empty until the authority answers.
    await expectIntents(alicePage, "roll_dice", {
      expr: "1d20+7",
      secret: false,
    });
    await expect(alicePage.locator(".chat-log .msg")).toHaveCount(0);

    // The authority (this test) broadcasts the roll entry to both pages.
    const entry = chatEntry({
      from: fixturePlayerName,
      kind: "roll",
      text: "attacks the skeleton",
      roll: { expr: "1d20+7", total: 24, rolls: [17], kept: [17] },
    });
    await emitToBoth(gmPage, alicePage, {
      session_id: fixtureSessionId,
      kind: "chat_appended",
      entry,
    });

    for (const p of [gmPage, alicePage]) {
      const log = p.locator(".chat-log");
      const msgs = log.locator('.msg[data-kind="roll"]');
      await expect(msgs).toHaveCount(1);
      await expect(msgs.locator(".from")).toHaveText(fixturePlayerName);
      await expect(log).toContainText("🎲 1d20+7 = 24");
      await expect(log).toContainText("Rolls: 17");
      // kept == rolls → no "Kept:" segment.
      await expect(log).not.toContainText("Kept:");
    }
    await shot(alicePage, "gameflow/07-first-roll-player");
    await shot(gmPage, "gameflow/08-first-roll-gm");
  } finally {
    await close();
  }
});

test("S20 secret-gm-roll", async ({ browser }) => {
  const { gmPage, alicePage, close } = await hostPair(browser);
  try {
    await waitForListener(gmPage, "session:patch");
    await waitForListener(gmPage, "session:presence");
    await waitForListener(alicePage, "session:patch");
    await waitForListener(alicePage, "session:presence");
    await emitSnapshotFromGm(gmPage, alicePage, {
      players: fixturePlayersGmAndAlice,
    });

    // GM tab → Dice sub-tab → custom expression → Roll (Secret).
    await switchTab(gmPage, "GM");
    await gmPage
      .locator(".gm-drawer-full .drawer-tabs")
      .getByRole("tab", { name: "Dice" })
      .click();
    await gmPage
      .locator('.dice-tools input[placeholder="Custom: 3d6+2"]')
      .fill("2d6+3");
    await gmPage.getByRole("button", { name: "Roll (Secret)" }).click();

    // Sending side: the intent carries secret: true, literally.
    await expectIntents(gmPage, "roll_dice", { expr: "2d6+3", secret: true });

    // The authority routes the secret roll entry to the GM page ONLY…
    const secret = chatEntry({
      from: fixtureGmName,
      kind: "roll",
      text: "secretly tests the lock",
      roll: { expr: "2d6+3", total: 9, rolls: [5, 4], kept: [5, 4] },
    });
    await emitPatch(gmPage, {
      session_id: fixtureSessionId,
      kind: "chat_appended",
      entry: secret,
    });
    // …then a public GM chat line to everyone — the ordering witness.
    const pub = chatEntry({
      from: fixtureGmName,
      kind: "chat",
      text: "The door clicks open.",
      roll: null,
    });
    await emitToBoth(gmPage, alicePage, {
      session_id: fixtureSessionId,
      kind: "chat_appended",
      entry: pub,
    });

    // GM DOM: both entries present (secret roll + public line).
    await switchTab(gmPage, "Chat+Dice");
    const gmLog = gmPage.locator(".chat-log");
    await expect(gmLog.locator(".msg")).toHaveCount(2);
    await expect(gmLog).toContainText("🎲 2d6+3 = 9");
    await expect(gmLog).toContainText("secretly tests the lock");
    await expect(gmLog).toContainText("The door clicks open.");
    await shot(gmPage, "gameflow/09-secret-roll-gm");

    // Player DOM — the positive witness pair: the public line rendered…
    const aliceLog = alicePage.locator(".chat-log");
    await expect(aliceLog).toContainText("The door clicks open.");
    // …and it is the ONLY entry: the secret roll never reached this DOM
    // (not "we forgot to send it" — the GM side above proves it exists).
    await expect(aliceLog.locator(".msg")).toHaveCount(1);
    await expect(aliceLog).not.toContainText("2d6+3");
    await expect(aliceLog).not.toContainText("secretly");
    await shot(alicePage, "gameflow/10-secret-roll-player-pov");
  } finally {
    await close();
  }
});

test("S21 handout-reveal", async ({ browser }) => {
  const { gmPage, alicePage, close } = await hostPair(browser);
  try {
    await waitForListener(gmPage, "session:patch");
    await waitForListener(gmPage, "session:presence");
    await waitForListener(alicePage, "session:patch");
    await waitForListener(alicePage, "session:presence");
    await emitSnapshotFromGm(gmPage, alicePage, {
      players: fixturePlayersGmAndAlice,
      handouts: fixtureHandoutsGmOnly,
    });

    // GM Handouts tab → Reveal to Players → the exact intent.
    await switchTab(gmPage, "Handouts");
    await expect(gmPage.locator(".handout-card h4")).toHaveText("letter");
    await gmPage.getByRole("button", { name: "Reveal to Players" }).click();
    await expectIntents(gmPage, "reveal_handout", { handout_id: "letter" });

    // The authority broadcasts the reveal to both pages.
    await emitToBoth(gmPage, alicePage, {
      session_id: fixtureSessionId,
      kind: "handout_revealed",
      handout_id: "letter",
    });

    // GM view: card remains, reveal affordance is gone (state change).
    await expect(gmPage.locator(".handout-card h4")).toHaveText("letter");
    await expect(
      gmPage.getByRole("button", { name: "Reveal to Players" }),
    ).toHaveCount(0);
    await shot(gmPage, "gameflow/11-handout-revealed-gm");

    // Player view: the handout is now listed.
    await switchTab(alicePage, "Handouts");
    await expect(alicePage.locator(".handout-card h4")).toHaveText("letter");
    await expect(
      alicePage.locator(".handouts-panel p.empty"),
    ).toHaveCount(0);
    await shot(alicePage, "gameflow/12-handout-revealed-player");
  } finally {
    await close();
  }
});

test("S22 initiative-advance", async ({ browser }) => {
  const { gmPage, alicePage, close } = await hostPair(browser);
  try {
    await waitForListener(gmPage, "session:patch");
    await waitForListener(gmPage, "session:presence");
    await waitForListener(alicePage, "session:patch");
    await waitForListener(alicePage, "session:presence");
    await emitSnapshotFromGm(gmPage, alicePage, {
      players: fixturePlayersGmAndAlice,
    });

    // GM Initiative tab: add Alice via "— Add to initiative —"…
    await switchTab(gmPage, "Initiative");
    await gmPage
      .locator(".add-to-init select")
      .selectOption({ label: fixturePlayerName });
    await gmPage
      .locator(".add-to-init")
      .getByRole("button", { name: "Add" })
      .click();
    // …the authority echoes the initiative patch back (the UI holds no
    // optimistic order — applyPatch only runs on broadcast patches)…
    await emitToBoth(gmPage, alicePage, {
      session_id: fixtureSessionId,
      kind: "initiative",
      order: [fixturePlayerPeerId],
      active_idx: 0,
      round: 0,
    });
    // …then the GM adds self; THIS set_initiative carries the cumulative
    // order, exactly the shape a live server round-trip produces.
    await gmPage
      .locator(".add-to-init select")
      .selectOption({ label: fixtureGmName });
    await gmPage
      .locator(".add-to-init")
      .getByRole("button", { name: "Add" })
      .click();
    await emitToBoth(gmPage, alicePage, {
      session_id: fixtureSessionId,
      kind: "initiative",
      order: [fixturePlayerPeerId, fixtureGmPeerId],
      active_idx: 0,
      round: 0,
    });
    await gmPage.getByRole("button", { name: "▶ Next Turn" }).click();

    // Sending side: exact intent sequence — two set_initiative (the second
    // cumulative), then advance_initiative.
    const kinds = (await invocations(gmPage))
      .filter((r) => r.cmd === "session_intent")
      .map((r) => (r.args as { intent: Intent }).intent.kind);
    expect(kinds).toEqual([
      "set_initiative",
      "set_initiative",
      "advance_initiative",
    ]);
    await expectIntents(gmPage, "set_initiative", {
      order: [fixturePlayerPeerId, fixtureGmPeerId],
    });

    // The authority advances the turn (Alice up, GM now active).
    await emitToBoth(gmPage, alicePage, {
      session_id: fixtureSessionId,
      kind: "initiative",
      order: [fixturePlayerPeerId, fixtureGmPeerId],
      active_idx: 1,
      round: 1,
    });

    // Both pages: two rows, the active highlight moved to the second row.
    await switchTab(alicePage, "Initiative");
    for (const p of [gmPage, alicePage]) {
      const rows = p.locator(".init-list li");
      await expect(rows).toHaveCount(2);
      await expect(rows.nth(0).locator(".name")).toHaveText(
        fixturePlayerName,
      );
      await expect(rows.nth(1).locator(".name")).toHaveText(fixtureGmName);
      await expect(rows.nth(0)).not.toHaveClass(/active/);
      await expect(rows.nth(1)).toHaveClass(/active/);
      await expect(p.locator(".round")).toHaveText("Round 1");
    }
    // The player's initiative surface has no GM controls.
    await expect(
      alicePage.getByRole("button", { name: "▶ Next Turn" }),
    ).toHaveCount(0);
    await shot(gmPage, "gameflow/13-initiative-advanced");
  } finally {
    await close();
  }
});

test("S23 end-session-teardown", async ({ browser }) => {
  const { gmPage, alicePage, close } = await hostPair(browser);
  try {
    await waitForListener(gmPage, "session:patch");
    await waitForListener(gmPage, "session:presence");
    await waitForListener(alicePage, "session:patch");
    await waitForListener(alicePage, "session:presence");
    await emitSnapshotFromGm(gmPage, alicePage, {
      players: fixturePlayersGmAndAlice,
    });

    // Native confirm(): Playwright auto-DISMISSES an unhandled dialog, which
    // would silently swallow the intent — the accept handler must be
    // registered before the click (§8 risk 4).
    gmPage.on("dialog", (d) => void d.accept());
    await switchTab(gmPage, "GM");
    await gmPage
      .locator(".gm-actions")
      .getByRole("button", { name: "End Session" })
      .click();
    await expectIntents(gmPage, "end_session");

    // The authority broadcasts the teardown to both pages.
    await emitToBoth(gmPage, alicePage, {
      session_id: fixtureSessionId,
      kind: "session_ended",
    });

    // The GM ended the session from the GM tab; bring the GM page back to
    // the chat surface for the drained-state sweep (Alice is already there).
    await switchTab(gmPage, "Chat+Dice");
    for (const p of [gmPage, alicePage]) {
      // Drained post-session surface (session.svelte.ts session_ended
      // branch): title falls back, badge reverts to "Player", the GM tab
      // disappears, panels are empty, no error surfaces fire.
      await expect(p.getByRole("heading", { level: 1 })).toHaveText(
        "Live Table",
      );
      await expect(p.locator(".badge")).toHaveText("Player");
      await expect(p.locator(".badge.gm")).toHaveCount(0);
      await expect(p.locator('.play-tabs [role="tab"]')).toHaveCount(5);
      await expect(p.locator(".chat-log p.empty")).toContainText(
        "No messages yet",
      );
      await expect(p.locator(".adventure-badge")).toHaveCount(0);
      await expect(p.getByRole("alert")).toHaveCount(0);
      // Party tab drains too: no cards, and no GM-only hint (the role that
      // gated it was wiped with the rest of the session state).
      await switchTab(p, "Party");
      await expect(p.locator(".party-card")).toHaveCount(0);
      await expect(p.locator(".party-panel p.empty")).toHaveCount(0);
    }
    await switchTab(gmPage, "Chat+Dice");
    await shot(gmPage, "gameflow/14-session-ended");
  } finally {
    await close();
  }
});

// The real resume flow §4's old resume-gap row promised ("when a resume UI
// lands, replace this test"). The fork merge landed it: the lobby's onMount
// refreshSaved()→session_list_saved fills a Saved Sessions panel, each row's
// Resume runs handleResume → session_load(campaign, session_id, name) →
// SessionInfo → transport hookup → goto /table/play. S24 drives exactly
// that, post-S23: the ended fixture session is the one listed.
test("S24 resume-saved-session", async ({ browser }) => {
  const context = await browser.newContext();
  const page = await context.newPage();
  try {
    await installHarness(page, {
      session_host: { ok: fixtureSessionInfoGm },
      session_intent: { ok: null },
      session_load: { ok: fixtureSessionInfoGm },
    });
    // Narrative per §4: "after S23's end-session state, return to /table".
    // Host and head back — the play page's onDestroy wipes session state on
    // the way out, so this is the true post-teardown lobby surface.
    await hostViaLobby(page, { name: fixtureGmName });

    // The Rust side has since persisted the ended session: re-stub
    // session_list_saved to surface it (same-page-flow re-stub via
    // __PF2E_HARNESS_CONFIG__.responses — the PLAN §9 12.0 mechanism; the
    // BASE stub keeps the list empty until now).
    await page.evaluate((saved) => {
      if (window.__PF2E_HARNESS_CONFIG__) {
        window.__PF2E_HARNESS_CONFIG__.responses.session_list_saved = {
          ok: [saved],
        };
      }
    }, fixtureSavedSession);

    // Return to the lobby the way a user goes (client-side tab-bar nav —
    // the remount re-runs refreshSaved with the new stub).
    await page.locator('.tabbar a[href="/table"]').click();

    // The Saved Sessions panel lists exactly the one ended fixture session,
    // described exactly as describeSavedSession formats it (name —
    // campaign • adventure • date-of-created_at).
    await expect(
      page.getByRole("heading", { level: 2, name: "Saved Sessions" }),
    ).toBeVisible();
    const savedRow = page.locator(".saved-list li");
    await expect(savedRow).toHaveCount(1);
    await expect(savedRow.locator(".saved-desc")).toHaveText(
      `${fixtureSessionName} — millbrook • widows-lamp • 2023-11-14`,
    );
    await expect(savedRow.locator(".badge-ended")).toHaveText("ended");
    // Host/join panels still render alongside (S13–S15 surfaces unchanged).
    await expect(
      page.getByRole("heading", { level: 2, name: "Host a Session" }),
    ).toBeVisible();
    await expect(
      page.getByRole("heading", { level: 2, name: "Join a Session" }),
    ).toBeVisible();

    // Resume: the round-trip auto-navigates to the play surface (the helper
    // also awaits both listener registrations).
    await resumeViaLobby(page, { name: fixtureGmName });

    // session_load carried the exact identity triple the Rust authority
    // expects — campaign id, session id, and the re-entered player name.
    const loads = (await invocations(page)).filter(
      (r) => r.cmd === "session_load",
    );
    expect(loads).toHaveLength(1);
    expect(loads[0]?.args).toEqual({
      campaignId: "millbrook",
      sessionId: fixtureSessionId,
      name: fixtureGmName,
    });

    // The authority re-emits the resumed session's snapshot (roster +
    // adventure, per the saved fixture); the play surface repopulates.
    await emitSnapshotFromGm(page, null, {
      players: fixturePlayersGmOnly,
      adventureSlug: "widows-lamp",
    });
    await expect(
      page.getByRole("heading", { level: 1, name: fixtureSessionName }),
    ).toBeVisible();
    await expect(page.locator(".badge.gm")).toHaveText("GM");
    await expect(page.locator(".adventure-badge")).toHaveText(
      "📖 widows-lamp",
    );
    await switchTab(page, "Party");
    await expect(page.locator(".party-card")).toHaveCount(1);
    await expect(
      page.locator(".party-card .card-header .name"),
    ).toHaveText(fixtureGmName);

    await switchTab(page, "Chat+Dice");
    await shot(page, "gameflow/15-resumed-session");
  } finally {
    await context.close();
  }
});
