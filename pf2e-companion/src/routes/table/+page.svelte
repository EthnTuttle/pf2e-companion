<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import { goto } from "$app/navigation";
  import { sessionHost, sessionJoinPeer, sessionLoad, type SessionInfo } from "$lib/ipc";
  import { campaignState, refreshCampaigns } from "$lib/campaign.svelte";
  import { lensState } from "$lib/lens.svelte";
  import { createTransport } from "$lib/transport";
  import { sessionState, resetSession, applyPatch, applyPresence } from "$lib/session.svelte";

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

  async function handleLoad(e: Event) {
    e.preventDefault();
    if (!campaignState.active) return;
    loading = true;
    error = null;
    try {
      // We'd need a session picker - for now just show an error
      error = "Session load not yet implemented - pick a session ID";
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(async () => {
    await refreshCampaigns();
    // Pre-fill player name from any stored preference
    const stored = localStorage.getItem("pf2e.playerName");
    if (stored) playerName = stored;
    const storedColor = localStorage.getItem("pf2e.playerColor");
    if (storedColor) playerColor = storedColor;
  });

  function savePlayerPrefs() {
    localStorage.setItem("pf2e.playerName", playerName);
    localStorage.setItem("pf2e.playerColor", playerColor);
  }

  $effect(() => {
    savePlayerPrefs();
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
    <a class="btn" href="/table/play">Enter Table →</a>
  </section>
{/if}

<section class="panel">
  <header><h2>Join a Session</h2></header>
  <div class="join-tabs">
    <button type="button" class:active={joinMode === "ticket"} onclick={() => (joinMode = "ticket")}>P2P Ticket</button>
    <button type="button" class:active={joinMode === "lan"} onclick={() => (joinMode = "lan")}>LAN URL</button>
  </div>
  <form onsubmit={handleJoin}>
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