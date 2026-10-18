<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import { goto } from "$app/navigation";
  import {
    sessionHost,
    sessionJoinPeer,
    sessionLoad,
    sessionListSaved,
    type SavedSessionSummary,
    type SessionInfo,
  } from "$lib/ipc";
  import { campaignState, refreshCampaigns } from "$lib/campaign.svelte";
  import { lensState } from "$lib/lens.svelte";
  import { buildInviteUri, inviteFromLobbyQuery } from "$lib/deeplink";
  import { createTransport } from "$lib/transport";
  import { sessionState, resetSession, applyPatch, applyPresence } from "$lib/session.svelte";
  import { describeSavedSession, exportSessionLogViaDialog } from "$lib/savedSessions.svelte";

  let playerName = $state("");
  let playerColor = $state("#3b82f6");
  let sessionName = $state("New Session");
  let adventureSlug = $state<string | null>(null);
  let joinTicket = $state("");
  let joinMode = $state<"ticket" | "lan">("ticket");
  let lanHost = $state("");
  let lanPin = $state("");
  let loading = $state(false);
  let error = $state<string | null>(null);
  let myTicket = $state<string | null>(null);
  let myLanUrl = $state<string | null>(null);
  let savedSessions = $state<SavedSessionSummary[]>([]);

  async function refreshSaved() {
    try {
      savedSessions = await sessionListSaved();
    } catch {
      savedSessions = [];
    }
  }

  const sessionPin = $derived(sessionState.info?.pin ?? null);

  const colors = [
    "#3b82f6", "#ef4444", "#22c55e", "#f59e0b", "#a855f7",
    "#ec4899", "#06b6d4", "#84cc16", "#f97316", "#6366f1",
  ];

  async function refreshAdventures() {
    // For now, hardcode the adventure we know exists
    // Later this could call an IPC to list adventures
  }

  function copyToClipboard(text: string, label: string) {
    navigator.clipboard.writeText(text);
    error = `Copied ${label} to clipboard`;
    setTimeout(() => (error = null), 2000);
  }

  // Tier 1 item 2 — one-tap invite: `pf2e://join?url=…&pin=…` opens the app
  // with the join form prefilled, replacing read-aloud host:port + PIN.
  function copyInviteLink() {
    if (!myLanUrl) return;
    const uri = buildInviteUri({ mode: "lan", lanUrl: myLanUrl, ticket: null, pin: sessionPin });
    if (uri) copyToClipboard(uri, "invite link");
  }

  async function handleHost(e: Event) {
    e.preventDefault();
    if (!playerName.trim() || !campaignState.active) return;
    loading = true;
    error = null;
    try {
      const info = await sessionHost(campaignState.active, sessionName, adventureSlug);
      sessionState.info = info;
      sessionState.connected = true;
      myTicket = info.p2p_ticket;
      myLanUrl = info.lan_url;
      void refreshSaved();

      const transport = createTransport(playerName);
      await transport.connect();
      transport.onPatch(applyPatch);
      transport.onPresence(applyPresence);

      goto("/table/play");
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function handleJoin(e: Event) {
    e.preventDefault();
    if (!playerName.trim()) return;
    loading = true;
    error = null;
    try {
      let info: SessionInfo;
      if (joinMode === "ticket") {
        if (!joinTicket.trim()) {
          error = "Enter a ticket";
          return;
        }
        info = await sessionJoinPeer(joinTicket, playerName);
      } else {
        if (!lanHost.trim()) {
          error = "Enter host LAN URL";
          return;
        }
        const url = new URL(lanHost);
        const sessionId = url.pathname.replace("/ws/", "").replace("/", "");
        const { invoke } = await import("@tauri-apps/api/core");
        info = await invoke("session_join_lan", { hostUrl: lanHost, name: playerName, sessionId, pin: lanPin.trim().toUpperCase() || null });
      }
      sessionState.info = info;
      sessionState.connected = true;
      myTicket = info.p2p_ticket;
      myLanUrl = info.lan_url;

      const transport = createTransport(playerName);
      await transport.connect();
      transport.onPatch(applyPatch);
      transport.onPresence(applyPresence);

      goto("/table/play");
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function handleResume(s: SavedSessionSummary) {
    if (!playerName.trim()) {
      error = "Enter your name first";
      return;
    }
    loading = true;
    error = null;
    try {
      const info = await sessionLoad(s.campaign, s.session_id, playerName);
      sessionState.info = info;
      sessionState.connected = true;
      myTicket = info.p2p_ticket;
      myLanUrl = info.lan_url;

      const transport = createTransport(playerName);
      await transport.connect();
      transport.onPatch(applyPatch);
      transport.onPresence(applyPresence);

      goto("/table/play");
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  // Pre-fill happens synchronously BEFORE the first await; the persist
  // effect below is gated on prefsLoaded so its initial run can't clobber
  // stored prefs with blank defaults (12.2-found race).
  let prefsLoaded = false;

  async function handleExportLog(s: SavedSessionSummary) {
    error = null;
    try {
      const path = await exportSessionLogViaDialog(s);
      if (path === null) {
        error = "Export cancelled";
        setTimeout(() => (error = null), 2000);
      } else {
        copyToClipboard(path, "export path");
      }
    } catch (e) {
      error = String(e);
    }
  }

  onMount(async () => {
    // Invite deep-link prefill (Tier 1 item 2): the layout navigates to
    // `/table?join=1&url=…&pin=…` (or `&ticket=…`) — prefill the join form
    // here, then scrub the params from history state so a PIN never
    // lingers in the app's navigation history. Join is still manual;
    // nothing auto-connects.
    const invite = inviteFromLobbyQuery(page.url.searchParams);
    if (invite) {
      joinMode = invite.mode;
      if (invite.lanUrl) lanHost = invite.lanUrl;
      if (invite.ticket) joinTicket = invite.ticket;
      if (invite.pin) lanPin = invite.pin;
      goto("/table", { replaceState: true });
    }
    const stored = localStorage.getItem("pf2e.playerName");
    if (stored) playerName = stored;
    const storedColor = localStorage.getItem("pf2e.playerColor");
    if (storedColor) playerColor = storedColor;
    prefsLoaded = true;
    await refreshCampaigns();
    await refreshSaved();
  });

  function savePlayerPrefs() {
    localStorage.setItem("pf2e.playerName", playerName);
    localStorage.setItem("pf2e.playerColor", playerColor);
  }

  $effect(() => {
    if (prefsLoaded) savePlayerPrefs();
  });
</script>

<header class="hero">
  <h1>Live Table</h1>
  <p class="tagline">Host or join a shared PF2e session. Real-time sync via P2P (desktop) or LAN WebSocket (mobile).</p>
</header>

{#if error}
  <p class="err">{error}</p>
{/if}

<section class="panel">
  <header><h2>Your Identity</h2></header>
  <div class="fields">
    <label>
      <span>Name</span>
      <input type="text" bind:value={playerName} placeholder="Your name" maxlength="32" />
    </label>
    <label>
      <span>Color</span>
      <div class="color-picker">
        <input type="color" bind:value={playerColor} />
        <div class="swatches">
          {#each colors as c}
            <button type="button" class="swatch" style="background:{c}" class:active={playerColor === c} onclick={() => (playerColor = c)} aria-label="Color {c}"></button>
          {/each}
        </div>
      </div>
    </label>
  </div>
</section>

<section class="panel">
  <header><h2>Host a Session</h2></header>
  <form onsubmit={handleHost}>
    <div class="fields">
      <label>
        <span>Session Name</span>
        <input type="text" bind:value={sessionName} placeholder="The Widow's Lamp — Session 1" />
      </label>
      <label>
        <span>Adventure (optional)</span>
        <select bind:value={adventureSlug}>
          <option value={null}>— None —</option>
          <option value="widows-lamp">The Widow's Lamp (Level 1)</option>
        </select>
      </label>
    </div>
    <button type="submit" class="primary" disabled={loading || !playerName.trim() || !campaignState.active}>
      {loading ? "Starting…" : "Host Session"}
    </button>
  </form>
  {#if campaignState.campaigns.length === 0}
    <p class="muted">Create a campaign in the <a href="/vault">Vault</a> first.</p>
  {/if}
</section>

{#if sessionState.info && sessionState.connected}
  <section class="panel invite">
    <header><h2>Invite Players</h2></header>
    <p class="muted">Share one of these with your players:</p>
    <div class="invite-row">
      <label>
        <span>P2P Ticket (desktop)</span>
        <div class="copy-row">
          <code>{myTicket}</code>
          <button type="button" onclick={() => copyToClipboard(myTicket!, "P2P ticket")}>Copy</button>
        </div>
      </label>
    </div>
    <div class="invite-row">
      <label>
        <span>LAN URL (mobile)</span>
        <div class="copy-row">
          <code>{myLanUrl}</code>
          <button type="button" onclick={() => copyToClipboard(myLanUrl!, "LAN URL")}>Copy</button>
        </div>
      </label>
    </div>
    {#if sessionPin}
    <div class="invite-row">
      <label>
        <span>Table PIN (required for LAN join)</span>
        <div class="copy-row">
          <code>{sessionPin}</code>
          <button type="button" onclick={() => copyToClipboard(sessionPin, "Table PIN")}>Copy</button>
        </div>
      </label>
    </div>
    {/if}
    {#if myLanUrl}
    <div class="invite-row">
      <label>
        <span>Invite link (text it — opens the app with the join form prefilled){sessionPin ? "; carries the PIN" : ""}</span>
        <div class="copy-row">
          <button type="button" onclick={copyInviteLink}>Copy invite link</button>
        </div>
      </label>
    </div>
    {/if}
    <a class="btn" href="/table/play">Enter Table →</a>
  </section>
{/if}

<section class="panel">
  <header><h2>Join a Session</h2></header>
  <div class="join-tabs">
    <button type="button" class:active={joinMode === "ticket"} onclick={() => (joinMode = "ticket")}>P2P Ticket</button>
    <button type="button" class:active={joinMode === "lan"} onclick={() => (joinMode = "lan")}>LAN URL</button>
  </div>
  <form class="join-form" onsubmit={handleJoin}>
    <div class="fields">
      {#if joinMode === "ticket"}
        <label>
          <span>Ticket</span>
          <input type="text" bind:value={joinTicket} placeholder="base32 ticket string…" />
        </label>
      {:else}
        <label>
          <span>Host LAN URL</span>
          <input type="url" bind:value={lanHost} placeholder="ws://192.168.1.42:4780/abc123" />
        </label>
        <label>
          <span>Table PIN (6 chars)</span>
          <input type="text" bind:value={lanPin} placeholder="e.g. A7K9M2" maxlength="6" style="text-transform: uppercase;" />
        </label>
      {/if}
    </div>
    <button type="submit" class="primary" disabled={loading || !playerName.trim()}>
      {loading ? "Joining…" : "Join Session"}
    </button>
  </form>
</section>

{#if savedSessions.length > 0}
  <section class="panel">
    <header><h2>Saved Sessions</h2></header>
    <ul class="saved-list">
      {#each savedSessions as s (s.session_id)}
        <li>
          <span class="saved-desc">{describeSavedSession(s)}</span>
          {#if s.ended}<span class="badge-ended">ended</span>{/if}
          <span class="saved-actions">
            <button type="button" onclick={() => handleResume(s)} disabled={loading || !playerName.trim()}>Resume</button>
            <button type="button" onclick={() => handleExportLog(s)}>Export log</button>
          </span>
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  .hero {
    margin-bottom: 1rem;
  }
  .hero h1 {
    margin: 0;
    font-size: 1.4rem;
    font-weight: 600;
  }
  .tagline {
    margin: 0.15rem 0 0;
    color: var(--muted);
    font-size: 0.85rem;
  }

  .panel {
    margin-top: 1.25rem;
    padding: 1rem;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--bg-soft);
  }
  .panel header {
    margin-bottom: 0.75rem;
  }
  .panel h2 {
    margin: 0;
    font-size: 0.9rem;
    font-weight: 600;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }

  .fields {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .fields label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.8rem;
    color: var(--muted);
  }
  .fields input,
  .fields select {
    font: inherit;
    padding: 0.5rem 0.7rem;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    color: inherit;
  }
  .color-picker {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .swatches {
    display: flex;
    gap: 0.35rem;
    flex-wrap: wrap;
  }
  .swatch {
    width: 1.75rem;
    height: 1.75rem;
    border-radius: 8px;
    border: 2px solid transparent;
    cursor: pointer;
  }
  .swatch.active {
    border-color: color-mix(in srgb, currentColor 50%, transparent);
  }
  .swatch:hover {
    transform: scale(1.1);
  }

  button.primary {
    font: inherit;
    font-weight: 500;
    padding: 0.6rem 1rem;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: color-mix(in srgb, currentColor 12%, transparent);
    color: inherit;
    cursor: pointer;
    margin-top: 0.5rem;
  }
  button.primary:hover:not([disabled]) {
    background: color-mix(in srgb, currentColor 18%, transparent);
  }
  button.primary[disabled] {
    opacity: 0.5;
    cursor: not-allowed;
  }
  /* F-R6 fix (ui-review 2026-10-18): on the native 390px viewport the lobby
     document is ~1100px tall; the Join submit's natural spot is doc
     y≈963–1003 — below the 867px viewport at rest, and mid-scroll inside
     the fixed tabbar's occlusion band (tabbar top edge at 814.8, h 52.17px).
     main's `padding-bottom: 5rem` only buys SCROLL clearance (max-scroll
     slack vs the tabbar is a constant ≈27.8px, matching F1's 27.6px); it
     cannot change the rest state, because content doc coords are padding-
     independent. `position: sticky; bottom: …` is NOT viable here: sticky
     travel is clamped to the containing block, and as the form's last child
     the button has only its own 8px top margin of room. Fixed CTA pinned
     above the tabbar (52px + 0.75rem) is the minimal working treatment —
     phone widths only: at ≥520px the lobby fits or the user can scroll to
     full clearance. The background must be opaque while pinned (stock
     button.primary is translucent) — recipe mirrors .tabbar in layout. */
  @media (max-width: 520px) {
    .join-form > button[type="submit"] {
      position: fixed;
      left: 2rem;
      right: 2rem;
      bottom: calc(52px + 0.75rem);
      z-index: 2;
      margin-top: 0;
      background: color-mix(in srgb, currentColor 12%, white 86%);
    }
  }
  @media (max-width: 520px) and (prefers-color-scheme: dark) {
    .join-form > button[type="submit"] {
      background: color-mix(in srgb, currentColor 14%, black 66%);
    }
  }

  .join-tabs {
    display: flex;
    gap: 0.25rem;
    margin-bottom: 0.6rem;
  }
  .join-tabs button {
    flex: 1;
    font: inherit;
    font-size: 0.8rem;
    padding: 0.4rem 0.6rem;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    color: var(--muted);
    cursor: pointer;
  }
  .join-tabs button.active {
    color: inherit;
    background: color-mix(in srgb, currentColor 12%, transparent);
    border-color: color-mix(in srgb, currentColor 30%, transparent);
  }

  .invite {
    border-color: color-mix(in srgb, hsl(140 60% 45%) 40%, var(--line));
  }
  .invite-row {
    margin-bottom: 0.75rem;
  }
  .invite-row label {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    font-size: 0.8rem;
    color: var(--muted);
  }
  .copy-row {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  .copy-row code {
    flex: 1;
    font-size: 0.75rem;
    background: color-mix(in srgb, currentColor 6%, transparent);
    padding: 0.35rem 0.5rem;
    border-radius: 6px;
    border: 1px solid var(--line);
    word-break: break-all;
  }
  .copy-row button {
    font: inherit;
    font-size: 0.75rem;
    padding: 0.35rem 0.7rem;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    color: inherit;
    cursor: pointer;
  }

  .muted {
    color: var(--muted);
    font-size: 0.85rem;
  }
  .muted a {
    color: inherit;
    text-decoration: underline;
  }

  .saved-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .saved-list li {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
    font-size: 0.85rem;
  }
  .saved-desc {
    flex: 1;
    min-width: 12rem;
  }
  .badge-ended {
    font-size: 0.7rem;
    padding: 0.1rem 0.4rem;
    border-radius: 999px;
    border: 1px solid var(--line);
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .saved-actions {
    display: flex;
    gap: 0.4rem;
  }
  .saved-actions button {
    font: inherit;
    font-size: 0.75rem;
    padding: 0.35rem 0.7rem;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    color: inherit;
    cursor: pointer;
  }
  .saved-actions button[disabled] {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .err {
    color: #c33;
    padding: 0.5rem 0.75rem;
    border: 1px solid #c33;
    border-radius: 6px;
    background: rgba(204, 51, 51, 0.05);
    font-size: 0.85rem;
    margin: 0.5rem 0;
  }

  .btn {
    display: inline-block;
    margin-top: 0.5rem;
    font: inherit;
    font-weight: 500;
    padding: 0.6rem 1rem;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: color-mix(in srgb, currentColor 12%, transparent);
    color: inherit;
    text-decoration: none;
  }
  .btn:hover {
    background: color-mix(in srgb, currentColor 18%, transparent);
  }
</style>