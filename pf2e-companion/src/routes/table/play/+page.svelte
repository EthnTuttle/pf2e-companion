<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { page } from "$app/state";
  import { goto } from "$app/navigation";
  import { type Intent, type Token, type Grid, type CharacterSheet, type RollRecord } from "$lib/ipc";
  import { sessionState, applyPatch, applyPresence, resetSession, clearError, setTransport, setConnectionState, retryConnection, getMyPeerId, getMyRole, getIsGm, getMyPlayer, getActiveInitiativePeer, getChatEntries, getTokens, getVisibleHandouts, getGrid, createSendIntent } from "$lib/session.svelte";
  import { createTransport } from "$lib/transport";
  import { lensState } from "$lib/lens.svelte";
  import EntityImage from "$lib/EntityImage.svelte";
  import {
    lensOfEntityId,
    packArtPath,
    resolveAdventureCover,
    resolveArtUrl,
    sceneVistaPath,
    type CoverInfo,
  } from "$lib/images";
  import { exportSessionLogViaDialog } from "$lib/savedSessions.svelte";

  let transport: ReturnType<typeof createTransport> | null = null;
  let activeTab = $state<"chat" | "initiative" | "party" | "scene" | "handouts" | "gm">("chat");
  let diceInput = $state("");
  let diceSecret = $state(false);
  let chatDraft = $state("");
  let showGmDrawer = $state(false);
  let gmDrawerInput = $state("");
  let error = $state<string | null>(null);
  let draggingToken: Token | null = null;
  let dragOffset = { x: 0, y: 0 };
  let sceneBgUrl = $state<string | null>(null);

  const myPeerId = $derived(getMyPeerId());
  const myRole = $derived(getMyRole());
  const isGm = $derived(getIsGm());
  const myPlayer = $derived(getMyPlayer());
  const activeInitiativePeer = $derived(getActiveInitiativePeer());
  const chatEntries = $derived(getChatEntries());
  const tokens = $derived(getTokens());
  const visibleHandouts = $derived(getVisibleHandouts());
  const grid = $derived(getGrid());

  const tabs = [
    { id: "chat", icon: "💬", label: "Chat+Dice", gmOnly: false },
    { id: "initiative", icon: "⚔️", label: "Initiative", gmOnly: false },
    { id: "party", icon: "👥", label: "Party", gmOnly: false },
    { id: "scene", icon: "🗺️", label: "Scene", gmOnly: false },
    { id: "handouts", icon: "📜", label: "Handouts", gmOnly: false },
    { id: "gm", icon: "🔧", label: "GM", gmOnly: true },
  ] as const;

  // F-R7: outbound intents ride the transport (wsTransport serializes §4.3
  // "intent" frames; ipcTransport calls session_intent itself) — NOT ipc.ts
  // directly. On a ws-joined page there is no session_intent IPC behind this
  // session, so every send must go through transport.send. Call sites stay
  // fire-and-forget; transport-level failures surface in the same error
  // banner the bridge's `error` frames already use.
  function sendIntent(intent: Intent): void {
    const t = transport;
    if (!t) return;
    void createSendIntent(t)(intent).catch((e: unknown) => {
      sessionState.error = e instanceof Error ? e.message : String(e);
    });
  }

  function rollDice(expr: string, secret = false) {
    sendIntent({ kind: "roll_dice", expr, secret } as Intent);
  }

  function sendChat() {
    if (!chatDraft.trim()) return;
    sendIntent({ kind: "chat", text: chatDraft.trim() } as Intent);
    chatDraft = "";
  }

  function parseDiceInput(input: string) {
    const match = input.match(/^\/roll\s+(.+)$/i);
    if (match) {
      const rest = match[1].trim();
      const secretMatch = rest.match(/^(.+)\s+--secret$/i);
      if (secretMatch) {
        rollDice(secretMatch[1], true);
      } else {
        rollDice(rest, false);
      }
      return true;
    }
    return false;
  }

  function handleChatSubmit(e: Event) {
    e.preventDefault();
    sendChat();
  }

  function handleChatKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      if (parseDiceInput(chatDraft)) {
        chatDraft = "";
      } else {
        sendChat();
      }
    }
  }

  function handleDiceKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      if (parseDiceInput("/roll " + diceInput)) {
        diceInput = "";
      } else {
        rollDice(diceInput, diceSecret);
        diceInput = "";
      }
    }
  }

  function advanceInitiative() {
    sendIntent({ kind: "advance_initiative" } as Intent);
  }

  function setInitiativeOrder(order: string[]) {
    sendIntent({ kind: "set_initiative", order } as Intent);
  }

  function revealHandout(handoutId: string) {
    sendIntent({ kind: "reveal_handout", handout_id: handoutId } as Intent);
  }

  function loadAdventure(slug: string = "widows-lamp") {
    sendIntent({ kind: "load_adventure", slug } as Intent);
  }

  function handleLoadAdventureClick(_e?: MouseEvent) {
    loadAdventure("widows-lamp");
  }

  function endSession(_e?: MouseEvent) {
    if (confirm("End this session for everyone? Export the session log first if you want a transcript.")) {
      sendIntent({ kind: "end_session" } as Intent);
    }
  }

  let exportStatus = $state<string | null>(null);

  async function exportSessionLog() {
    if (!isGm || !sessionState.info) return;
    exportStatus = null;
    try {
      const path = await exportSessionLogViaDialog({
        name: sessionState.snapshot?.meta.name ?? sessionState.info.name,
        session_id: sessionState.info.session_id,
      });
      if (path === null) {
        exportStatus = "Export cancelled";
      } else {
        navigator.clipboard.writeText(path);
        exportStatus = `Exported to ${path} (path copied to clipboard)`;
      }
    } catch (e) {
      exportStatus = String(e);
    }
  }

  function createCharacter() {
    if (!myPlayer?.character) {
      const sheet: CharacterSheet = {
        level: 1,
        class: "Fighter",
        hp: 10,
        max_hp: 10,
        ac: 16,
        conditions: [],
        notes: "",
      };
      sendIntent({
        kind: "create_character",
        name: sessionState.info?.name ?? "Player",
        color: "#3b82f6",
        character: sheet,
      } as Intent);
    }
  }

  function updateSheet(patch: Partial<CharacterSheet>) {
    sendIntent({ kind: "update_sheet", patch } as Intent);
  }

  function addToken() {
    if (!isGm) return;
    const token: Token = {
      token_id: crypto.randomUUID(),
      owner: myPeerId!,
      entity_id: null,
      pc: null,
      x: Math.floor(grid.w / 2),
      y: Math.floor(grid.h / 2),
      label: "Token",
    };
    sendIntent({ kind: "add_token", token } as Intent);
  }

  function removeToken(tokenId: string) {
    if (!isGm) return;
    sendIntent({ kind: "remove_token", token_id: tokenId } as Intent);
  }

  function handleTokenMouseDown(token: Token, e: MouseEvent) {
    const canMove = isGm || token.owner === myPeerId;
    if (!canMove) return;
    draggingToken = token;
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    dragOffset.x = e.clientX - rect.left;
    dragOffset.y = e.clientY - rect.top;
    e.preventDefault();
  }

  function handleMouseMove(e: MouseEvent) {
    if (!draggingToken) return;
    const container = document.querySelector(".grid-container") as HTMLElement;
    if (!container) return;
    const rect = container.getBoundingClientRect();
    const cellSize = rect.width / grid.w;
    const x = Math.floor((e.clientX - rect.left) / cellSize);
    const y = Math.floor((e.clientY - rect.top) / cellSize);
    const clampedX = Math.max(0, Math.min(grid.w - 1, x));
    const clampedY = Math.max(0, Math.min(grid.h - 1, y));
    if (clampedX !== draggingToken.x || clampedY !== draggingToken.y) {
      sendIntent({ kind: "move_token", token_id: draggingToken.token_id, x: clampedX, y: clampedY } as Intent);
    }
  }

  function handleMouseUp() {
    draggingToken = null;
  }

  function setSceneBackground(entityId: string | null) {
    if (!isGm) return;
    sendIntent({ kind: "set_scene", grid: { ...grid }, background_entity_id: entityId } as Intent);
  }

  function handleGmDrawerSubmit(e: Event) {
    e.preventDefault();
    if (gmDrawerInput.trim().startsWith("/roll ")) {
      rollDice(gmDrawerInput.trim().slice(6), true);
    } else {
      sendIntent({ kind: "chat", text: gmDrawerInput.trim() } as Intent);
    }
    gmDrawerInput = "";
  }

  function handleGmDrawerKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      if (gmDrawerInput.trim().startsWith("/roll ")) {
        rollDice(gmDrawerInput.trim().slice(6), true);
      } else {
        sendIntent({ kind: "chat", text: gmDrawerInput.trim() } as Intent);
      }
      gmDrawerInput = "";
    }
  }

  function moveInitiative(peerId: string, delta: number) {
    const order = [...(sessionState.snapshot?.init.order ?? [])];
    const idx = order.indexOf(peerId);
    if (idx === -1) return;
    const newIdx = idx + delta;
    if (newIdx < 0 || newIdx >= order.length) return;
    [order[idx], order[newIdx]] = [order[newIdx], order[idx]];
    setInitiativeOrder(order);
  }

  function removeFromInitiative(peerId: string) {
    const order = (sessionState.snapshot?.init.order ?? []).filter((p) => p !== peerId);
    setInitiativeOrder(order);
  }

  let initAddPeer = $state("");
  function addToInitiative(peerId: string) {
    if (!peerId) return;
    const order = [...(sessionState.snapshot?.init.order ?? [])];
    if (!order.includes(peerId)) {
      order.push(peerId);
      setInitiativeOrder(order);
    }
    initAddPeer = "";
  }

  let gmDrawerTab = $state<"chat" | "dice" | "tools">("chat");
  let gmDiceExpr = $state("");

  async function init() {
    if (sessionState.connected && sessionState.info) {
      // Lobby-first flow (host / ticket join / resume): the lobby already
      // connected a transport, attached the patch/presence handlers, and
      // stashed it (setTransport) — adopt it so this page's sends ride the
      // SAME backend the patches arrive on. A page that invents a second
      // backend here is exactly the F-R7 bug class.
      transport = sessionState.transport;
      return;
    }

    const params = new URLSearchParams(page.url.search);
    const name = params.get("name") ?? localStorage.getItem("pf2e.playerName") ?? "Player";

    try {
      transport = createTransport(name);
      transport.onStateChange((state, attempt) => {
        setConnectionState(state, attempt);
      });
      await transport.connect();
      transport.onPatch(applyPatch);
      transport.onPresence(applyPresence);
      transport.onError((msg) => {
        sessionState.error = msg;
      });
      sessionState.connected = true;
      setTransport(transport);
    } catch (e) {
      error = String(e);
      setTimeout(() => goto("/table"), 2000);
    }
  }

  onMount(() => {
    init();
    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
  });

  onDestroy(() => {
    window.removeEventListener("mousemove", handleMouseMove);
    window.removeEventListener("mouseup", handleMouseUp);
    transport?.close();
    resetSession();
  });

  // Imagery v1 (DEDUCTION §4.3.2): the doc's background_entity_id drives the
  // vista. `lens:adventure-<slug>[#scene-N]` → promoted scene art; anything
  // else leaves the placeholder grid up.
  $effect(() => {
    const path = sceneVistaPath(
      sessionState.snapshot?.scene.background_entity_id,
    );
    sceneBgUrl = null;
    if (!path) return;
    let live = true;
    void resolveArtUrl(path).then((u) => {
      if (live) sceneBgUrl = u;
    });
    return () => {
      live = false;
    };
  });

  // Imagery v1 (DEDUCTION §4.3.2 Load flow): cover panel beside the Load
  // action. The GM action hardcodes widows-lamp, so the preview does too.
  let loadCover = $state<CoverInfo | null>(null);
  $effect(() => {
    let live = true;
    void resolveAdventureCover(lensState.active, "widows-lamp").then((c) => {
      if (live) loadCover = c;
    });
    return () => {
      live = false;
    };
  });

  // Dismissible cover banner once an adventure is loaded; the text
  // `.adventure-badge` in the header stays as the fallback/indicator.
  let coverBanner = $state<CoverInfo | null>(null);
  let bannerDismissedFor = $state<string | null>(null);
  $effect(() => {
    const slug = sessionState.snapshot?.meta.adventure_slug;
    if (!slug || bannerDismissedFor === slug) {
      coverBanner = null;
      return;
    }
    let live = true;
    void resolveAdventureCover(lensState.active, slug).then((c) => {
      if (live) coverBanner = c;
    });
    return () => {
      live = false;
    };
  });

  // Token portraits: the authority stamps `token.image` at adventure-load
  // when the NPC's promoted portrait is bundled (DEDUCTION §4.3.2); empty →
  // the token keeps its flat-color fallback chip.
  function tokenArtPath(token: Token): string | null {
    return token.image && token.image.trim() ? token.image : null;
  }

  function handoutArtPath(entityId: string, image: string | undefined) {
    return packArtPath(lensOfEntityId(entityId), image);
  }
</script>

{#if error && !sessionState.connected}
  <div class="error-banner" role="alert">
    <p>{error}</p>
    <p class="muted">Redirecting to lobby…</p>
  </div>
{:else}
  <div class="play-surface">
    <header class="play-header">
      <h1>{sessionState.snapshot?.meta.name ?? "Live Table"}</h1>
      <div class="session-info">
        <span class="badge" class:gm={isGm}>{isGm ? "GM" : "Player"}</span>
        <span class="peer-id">{myPeerId?.slice(0, 8)}</span>
        {#if sessionState.snapshot?.meta.adventure_slug}
          <span class="adventure-badge">📖 {sessionState.snapshot.meta.adventure_slug}</span>
        {/if}
      </div>
    </header>

    {#if coverBanner}
      <figure class="cover-banner">
        <img src={coverBanner.url} alt={coverBanner.alt} />
        <button
          type="button"
          class="dismiss"
          aria-label="Dismiss adventure cover"
          onclick={() =>
            (bannerDismissedFor =
              sessionState.snapshot?.meta.adventure_slug ?? "")}
        >×</button>
      </figure>
    {/if}

    <div class="play-tabs" role="tablist" aria-label="Table tabs">
      {#each tabs as t}
        {#if !t.gmOnly || isGm}
          <button
            role="tab"
            aria-selected={activeTab === t.id}
            class:active={activeTab === t.id}
            onclick={() => (activeTab = t.id)}
          >
            <span class="ico" aria-hidden="true">{t.icon}</span>
            <span class="lbl">{t.label}</span>
          </button>
        {/if}
      {/each}
    </div>

    {#if sessionState.connectionState === "reconnecting"}
      <div class="connection-banner reconnecting" role="status" aria-live="polite">
        <span>Reconnecting… (attempt {sessionState.reconnectAttempt}/3)</span>
      </div>
    {:else if sessionState.connectionState === "failed"}
      <div class="connection-banner failed" role="alert">
        <span>Connection lost</span>
        <button onclick={retryConnection} class="rejoin-btn">Rejoin</button>
      </div>
    {:else if sessionState.error}
      <div class="session-error-banner" role="alert">
        <span>{sessionState.error}</span>
        <button onclick={clearError} aria-label="Dismiss">✕</button>
      </div>
    {/if}

    <main class="play-panel" aria-label={activeTab}>
      {#if activeTab === "chat"}
        <div class="chat-panel">
          <div class="chat-log">
            {#each chatEntries as entry (entry.ts)}
              <div class="msg" data-kind={entry.kind}>
                <span class="from">{entry.from}</span>
                <span class="text">{entry.text}</span>
                {#if entry.roll}
                  <details class="roll-detail">
                    <summary>🎲 {entry.roll.expr} = <strong>{entry.roll.total}</strong></summary>
                    <div class="roll-breakdown">
                      Rolls: {entry.roll.rolls.join(", ")}
                      {#if entry.roll.kept.length > 0 && entry.roll.kept.length < entry.roll.rolls.length}
                        | Kept: {entry.roll.kept.join(", ")}
                      {/if}
                    </div>
                  </details>
                {/if}
              </div>
            {:else}
              <p class="empty">No messages yet. Type below or use <kbd>/roll 1d20+7</kbd>.</p>
            {/each}
          </div>
          <div class="chat-input-row">
            <form onsubmit={handleChatSubmit}>
              <input
                type="text"
                bind:value={chatDraft}
                placeholder="Type a message or /roll 1d20+7"
                autocomplete="off"
                onkeydown={handleChatKeydown}
              />
            </form>
            <div class="dice-quick">
              <input
                type="text"
                bind:value={diceInput}
                placeholder="1d20+7"
                onkeydown={handleDiceKeydown}
                title="Quick roll (Enter to roll)"
              />
              <label class="secret-chk">
                <!-- F-R3: checkbox must use bind:checked (bind:value on a
                     checkbox throws bind_invalid_checkbox_value in Svelte dev
                     mode and aborts the play-page mount — this made the lobby
                     "Enter Table" anchor a no-op on the native desktop shell.
                     boolean semantics unchanged: rollDice(..., diceSecret) reads the same $state. -->
                <input type="checkbox" bind:checked={diceSecret} />
                Secret
              </label>
            </div>
          </div>
        </div>
      {:else if activeTab === "initiative"}
        <div class="initiative-panel">
          <header>
            <h3>Initiative</h3>
            <div class="controls">
              {#if isGm}
                <button onclick={advanceInitiative} class="primary">▶ Next Turn</button>
                <button onclick={() => setInitiativeOrder([])}>Clear</button>
              {/if}
              <span class="round">Round {sessionState.snapshot?.init.round ?? 1}</span>
            </div>
          </header>
          <ol class="init-list">
            {#each sessionState.snapshot?.init.order ?? [] as peerId, i}
              <li class:active={i === sessionState.snapshot?.init.active_idx}>
                <span class="num">{i + 1}.</span>
                <span class="name">{sessionState.snapshot?.players[peerId]?.name ?? peerId.slice(0, 8)}</span>
                {#if isGm}
                  <button class="move-btn" onclick={() => moveInitiative(peerId, -1)} disabled={i === 0} aria-label="Move up">↑</button>
                  <button class="move-btn" onclick={() => moveInitiative(peerId, 1)} disabled={i === (sessionState.snapshot?.init.order.length ?? 1) - 1} aria-label="Move down">↓</button>
                  <button class="move-btn" onclick={() => removeFromInitiative(peerId)} aria-label="Remove from initiative">✕</button>
                {/if}
              </li>
            {:else}
              <li class="empty">No initiative order. GM: add players via Party tab.</li>
            {/each}
          </ol>
          {#if isGm}
            <div class="add-to-init">
              <label>
                <select bind:value={initAddPeer}>
                  <option value="">— Add to initiative —</option>
                  {#each Object.entries(sessionState.snapshot?.players ?? {}) as [pid, p]}
                    <option value={pid}>{p.name}</option>
                  {/each}
                </select>
                <button onclick={() => addToInitiative(initAddPeer)} disabled={!initAddPeer}>Add</button>
              </label>
            </div>
          {/if}
        </div>
      {:else if activeTab === "party"}
        <div class="party-panel">
          {#each Object.entries(sessionState.snapshot?.players ?? {}) as [peerId, player] (peerId)}
            <div class="party-card" style="--player-color: {player.color};" class:dimmed={player.connected === false}>
              <div class="card-header">
                <span class="name">{player.name}</span>
                <span class="peer">{peerId.slice(0, 8)}</span>
                {#if player.connected === false}
                  <span class="reconnecting-badge">reconnecting…</span>
                {/if}
              </div>
              {#if player.character}
                <div class="sheet">
                  <div class="row">
                    <label>Class <input type="text" bind:value={player.character.class} onchange={() => updateSheet({ class: player.character!.class })} disabled={peerId !== myPeerId && !isGm} /></label>
                    <label>Level <input type="number" bind:value={player.character.level} min="1" max="20" onchange={() => updateSheet({ level: player.character!.level })} disabled={peerId !== myPeerId && !isGm} /></label>
                  </div>
                  <div class="row">
                    <label>HP <input type="number" bind:value={player.character.hp} min="0" onchange={() => updateSheet({ hp: player.character!.hp })} disabled={peerId !== myPeerId && !isGm} /> / <input type="number" bind:value={player.character.max_hp} min="1" onchange={() => updateSheet({ max_hp: player.character!.max_hp })} disabled={peerId !== myPeerId && !isGm} /></label>
                    <label>AC <input type="number" bind:value={player.character.ac} min="1" onchange={() => updateSheet({ ac: player.character!.ac })} disabled={peerId !== myPeerId && !isGm} /></label>
                  </div>
                  <div class="row full">
                    <label>Conditions <input type="text" value={player.character.conditions.join(", ")} oninput={(e) => { const arr = (e.target as HTMLInputElement).value.split(",").map(s => s.trim()).filter(Boolean); updateSheet({ conditions: arr }); }} placeholder="frightened 1, slowed" disabled={peerId !== myPeerId && !isGm} /></label>
                  </div>
                  <div class="row full">
                    <label>Notes <textarea bind:value={player.character.notes} onchange={() => updateSheet({ notes: player.character!.notes })} placeholder="Notes…" disabled={peerId !== myPeerId && !isGm} rows="2"></textarea></label>
                  </div>
                </div>
              {:else}
                <p class="no-sheet">No character created.</p>
                {#if peerId === myPeerId || isGm}
                  <button onclick={createCharacter} class="primary">Create Character</button>
                {/if}
              {/if}
            </div>
          {/each}
          {#if isGm && (!sessionState.snapshot || Object.keys(sessionState.snapshot.players).length === 0)}
            <p class="empty">No players yet. Share the invite from the lobby.</p>
          {/if}
        </div>
      {:else if activeTab === "scene"}
        <div class="scene-panel">
          <header>
            <h3>Scene</h3>
            {#if isGm}
              <div class="scene-controls">
                <button onclick={addToken}>+ Token</button>
                {#if sessionState.snapshot?.meta.adventure_slug}
                  <button
                    title="Use the adventure's scene art as the grid backdrop"
                    onclick={() =>
                      setSceneBackground(
                        `${lensState.active}:adventure-${sessionState.snapshot!.meta.adventure_slug}`,
                      )}
                  >Adventure BG</button>
                {/if}
                <button onclick={() => setSceneBackground(null)}>Clear BG</button>
              </div>
            {/if}
          </header>
          <div class="grid-container" style="--cell-size: 32px; --grid-w: {grid.w}; --grid-h: {grid.h};">
            {#if sceneBgUrl}
              <div class="grid-bg" style="background-image: url({sceneBgUrl})"></div>
            {:else}
              <div class="grid-bg placeholder"></div>
            {/if}
            <div class="grid-lines"></div>
            <div class="tokens-layer">
              {#each tokens as token}
                <div
                  class="token"
                  role="button"
                  tabindex="0"
                  aria-label="Token {token.label} at {token.x},{token.y}"
                  style="left: {token.x * 32}px; top: {token.y * 32}px; border-color: {token.owner === myPeerId ? '#22c55e' : (isGm ? '#f59e0b' : '#6b7280')};"
                  class:owned={token.owner === myPeerId}
                  class:gm-token={isGm && token.owner !== myPeerId}
                  onmousedown={(e) => handleTokenMouseDown(token, e)}
                  onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); handleTokenMouseDown(token, e as unknown as MouseEvent); } }}
                >
                  {#if tokenArtPath(token)}
                    <EntityImage
                      path={tokenArtPath(token)}
                      alt="{token.label} portrait"
                      class="token-art"
                    />
                  {/if}
                  <span class="label">{token.label}</span>
                  {#if (isGm || token.owner === myPeerId) && token.pc}
                    <span class="pc-badge">{token.pc}</span>
                  {/if}
                </div>
              {/each}
            </div>
          </div>
          <div class="scene-legend">
            <span>Grid: {grid.w}×{grid.h} ({grid.size}px)</span>
            {#if isGm}
              <span class="hint">Drag tokens to move. GM: click +Token to add.</span>
            {:else}
              <span class="hint">Drag your tokens to move.</span>
            {/if}
          </div>
        </div>
      {:else if activeTab === "handouts"}
        <div class="handouts-panel">
          {#each visibleHandouts as h}
            <article class="handout-card">
              <header>
                <h4>{h.id}</h4>
                {#if isGm && h.visible_to === "gm"}
                  <button onclick={() => revealHandout(h.id)}>Reveal to Players</button>
                {/if}
              </header>
              {#if h.image}
                <EntityImage
                  path={handoutArtPath(h.entity_id, h.image)}
                  alt={h.image_alt ?? ""}
                  class="handout-art"
                />
              {/if}
              <div class="handout-body">
                <p class="muted">Handout content for entity {h.entity_id} — markdown rendering TBD</p>
              </div>
            </article>
          {/each}
          {#if visibleHandouts.length === 0}
            <p class="empty">No handouts revealed yet.</p>
          {/if}
        </div>
      {:else if activeTab === "gm"}
        <div class="gm-drawer-full">
          <header>
            <h3>GM Drawer</h3>
            <div class="gm-actions">
              {#if loadCover}
                <img class="cover-thumb" src={loadCover.url} alt={loadCover.alt} />
              {/if}
              <button onclick={handleLoadAdventureClick} class="primary">Load Widow's Lamp</button>
              <button onclick={endSession} class="danger">End Session</button>
            </div>
          </header>
          {#if sessionState.error}
            <div class="gm-hint">Authority rejections appear in the red error banner above.</div>
          {/if}
          <div class="drawer-tabs" role="tablist" aria-label="GM drawer tabs">
            <button role="tab" aria-selected={gmDrawerTab === "chat"} class:active={gmDrawerTab === "chat"} onclick={() => (gmDrawerTab = "chat")}>Chat Log</button>
            <button role="tab" aria-selected={gmDrawerTab === "dice"} class:active={gmDrawerTab === "dice"} onclick={() => (gmDrawerTab = "dice")}>Dice</button>
            <button role="tab" aria-selected={gmDrawerTab === "tools"} class:active={gmDrawerTab === "tools"} onclick={() => (gmDrawerTab = "tools")}>Tools</button>
          </div>
          <div class="drawer-content" role="tabpanel">
            {#if gmDrawerTab === "chat"}
              <div class="drawer-chat">
                {#each chatEntries as entry (entry.ts)}
                  <div class="msg" data-kind={entry.kind}>
                    <span class="from">{entry.from}</span>
                    <span class="text">{entry.text}</span>
                    {#if entry.roll}
                      <span class="roll">🎲 {entry.roll.expr} = <strong>{entry.roll.total}</strong></span>
                    {/if}
                  </div>
                {/each}
              </div>
            {:else if gmDrawerTab === "dice"}
              <div class="dice-tools">
                <h4>Quick Rolls</h4>
                <div class="dice-grid">
                  <button onclick={() => rollDice("1d20", true)}>d20</button>
                  <button onclick={() => rollDice("1d20+5", true)}>d20+5</button>
                  <button onclick={() => rollDice("2d6", true)}>2d6</button>
                  <button onclick={() => rollDice("4d6k3", true)}>4d6k3</button>
                  <button onclick={() => rollDice("1d8+3", true)}>d8+3</button>
                  <button onclick={() => rollDice("2d10", true)}>2d10</button>
                </div>
                <form onsubmit={(e) => { e.preventDefault(); rollDice(gmDiceExpr, true); gmDiceExpr = ""; }}>
                  <input type="text" bind:value={gmDiceExpr} placeholder="Custom: 3d6+2" />
                  <button type="submit">Roll (Secret)</button>
                </form>
              </div>
            {:else if gmDrawerTab === "tools"}
              <div class="gm-tools">
                <h4>Session Tools</h4>
                <button onclick={exportSessionLog}>Export session log</button>
                {#if exportStatus}<p class="muted">{exportStatus}</p>{/if}
                <button onclick={endSession} class="danger">End Session</button>
              </div>
            {/if}
          </div>
          <form class="drawer-input" onsubmit={handleGmDrawerSubmit}>
            <input type="text" bind:value={gmDrawerInput} placeholder="/roll 2d6+3 --secret or chat message" onkeydown={handleGmDrawerKeydown} />
          </form>
        </div>
      {/if}
    </main>

    {#if showGmDrawer && isGm}
      <div class="gm-drawer-overlay" role="dialog" aria-modal="true" aria-label="GM Drawer" tabindex="-1" onkeydown={(e) => { if (e.key === 'Escape') showGmDrawer = false; }}>
        <div class="gm-drawer">
          <header>
            <h3>GM Drawer</h3>
            <button onclick={() => (showGmDrawer = false)} aria-label="Close">✕</button>
          </header>
          {#if sessionState.error}
            <div class="gm-hint">Authority rejections appear in the red error banner above.</div>
          {/if}
          <div class="drawer-content">
            <div class="drawer-chat">
              {#each chatEntries as entry (entry.ts)}
                <div class="msg" data-kind={entry.kind}>
                  <span class="from">{entry.from}</span>
                  <span class="text">{entry.text}</span>
                  {#if entry.roll}
                    <span class="roll">🎲 {entry.roll.expr} = <strong>{entry.roll.total}</strong> [{entry.roll.rolls.join(",")}]</span>
                  {/if}
                </div>
              {/each}
            </div>
            <form onsubmit={handleGmDrawerSubmit}>
              <input
                type="text"
                bind:value={gmDrawerInput}
                placeholder="/roll 2d6+3 --secret  or  chat message"
                onkeydown={handleGmDrawerKeydown}
              />
            </form>
          </div>
        </div>
      </div>
    {/if}
  </div>
{/if}

<style>
  .error-banner {
    padding: 1rem;
    background: rgba(204, 51, 51, 0.1);
    border: 1px solid #c33;
    border-radius: 8px;
    color: #c33;
    text-align: center;
  }
  .error-banner .muted {
    color: #a33;
    font-size: 0.85rem;
    margin-top: 0.5rem;
  }

  .session-error-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.5rem 1rem;
    background: rgba(204, 51, 51, 0.15);
    border-bottom: 1px solid #c33;
    color: #c33;
    font-size: 0.85rem;
    flex-shrink: 0;
  }
  .session-error-banner button {
    background: none;
    border: 1px solid #c33;
    color: #c33;
    border-radius: 4px;
    padding: 0.2rem 0.5rem;
    cursor: pointer;
    font-size: 0.8rem;
  }
  .session-error-banner button:hover {
    background: rgba(204, 51, 51, 0.1);
  }

  .connection-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.5rem 1rem;
    font-size: 0.85rem;
    flex-shrink: 0;
  }
  .connection-banner.reconnecting {
    background: rgba(245, 158, 11, 0.15);
    border-bottom: 1px solid #f59e0b;
    color: #f59e0b;
  }
  .connection-banner.failed {
    background: rgba(204, 51, 51, 0.15);
    border-bottom: 1px solid #c33;
    color: #c33;
  }
  .rejoin-btn {
    background: none;
    border: 1px solid currentColor;
    color: inherit;
    border-radius: 4px;
    padding: 0.2rem 0.5rem;
    cursor: pointer;
    font-size: 0.8rem;
  }
  .rejoin-btn:hover {
    background: color-mix(in srgb, currentColor 15%, transparent);
  }

  .gm-hint {
    padding: 0.5rem 1rem;
    background: rgba(255, 165, 0, 0.1);
    border-bottom: 1px solid #c80;
    color: #c80;
    font-size: 0.8rem;
    font-style: italic;
  }

  .party-card.dimmed {
    opacity: 0.6;
  }

  .play-surface {
    display: flex;
    flex-direction: column;
    /* F1 fix (ui-review 2026-10-14): fill main's content box — on this route
       +layout.svelte pins .app.fullbleed to the (dynamic) viewport height, so
       100% here already excludes the topbar, main padding, and the fixed
       tabbar. The old `height: 100vh` bypassed main's 5rem bottom clearance
       and parked the chat composer behind/below the tabbar at scroll 0
       (composer top 876px vs 844px viewport at 390px). */
    height: 100%;
    min-height: 0;
    overflow: hidden;
  }

  .play-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.5rem 1rem;
    border-bottom: 1px solid var(--line);
    background: var(--bg-soft);
    flex-shrink: 0;
  }
  .play-header h1 {
    margin: 0;
    font-size: 1.1rem;
    font-weight: 600;
  }
  .session-info {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.75rem;
  }
  .badge {
    padding: 0.15rem 0.4rem;
    border-radius: 999px;
    font-size: 0.65rem;
    font-weight: 600;
    text-transform: uppercase;
    background: var(--bg-soft);
    border: 1px solid var(--line);
    color: var(--muted);
  }
  .badge.gm {
    background: color-mix(in srgb, hsl(45 90% 55%) 15%, transparent);
    border-color: hsl(45 90% 55%);
    color: hsl(45 90% 40%);
  }
  .peer-id {
    font-family: ui-monospace, monospace;
    color: var(--muted);
  }
  .adventure-badge {
    font-size: 0.7rem;
    padding: 0.15rem 0.4rem;
    border-radius: 999px;
    background: color-mix(in srgb, hsl(260 60% 50%) 12%, transparent);
    border: 1px solid hsl(260 60% 50%);
    color: hsl(260 60% 60%);
  }

  .play-tabs {
    display: flex;
    border-bottom: 1px solid var(--line);
    background: var(--bg-soft);
    flex-shrink: 0;
    overflow-x: auto;
  }
  .play-tabs button {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.5rem 0.75rem;
    font: inherit;
    font-size: 0.75rem;
    color: var(--muted);
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    cursor: pointer;
    white-space: nowrap;
  }
  .play-tabs button.active {
    color: inherit;
    border-bottom-color: currentColor;
  }
  .play-tabs .ico { font-size: 1rem; }
  .play-tabs .lbl { display: none; }
  @media (min-width: 480px) {
    .play-tabs .lbl { display: inline; }
  }

  .play-panel {
    flex: 1;
    overflow: auto;
    padding: 1rem;
  }

  /* Chat Panel */
  .chat-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    gap: 0.5rem;
  }
  .chat-log {
    flex: 1;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    padding: 0.5rem;
    background: var(--bg-soft);
    border: 1px solid var(--line);
    border-radius: 8px;
  }
  .chat-log .msg {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    padding: 0.5rem 0.7rem;
    border-radius: 8px;
    background: color-mix(in srgb, currentColor 4%, transparent);
    border: 1px solid var(--line);
    font-size: 0.85rem;
    line-height: 1.4;
  }
  .chat-log .msg[data-kind="roll"] {
    border-color: color-mix(in srgb, hsl(45 90% 55%) 40%, var(--line));
    background: color-mix(in srgb, hsl(45 90% 55%) 6%, transparent);
  }
  .chat-log .msg[data-kind="system"] {
    color: var(--muted);
    font-style: italic;
    font-size: 0.8rem;
  }
  .chat-log .from {
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
    font-weight: 600;
  }
  .roll-detail summary {
    cursor: pointer;
    font-size: 0.8rem;
    font-family: ui-monospace, monospace;
  }
  .roll-breakdown {
    margin-top: 0.25rem;
    font-size: 0.75rem;
    color: var(--muted);
    font-family: ui-monospace, monospace;
  }
  .chat-input-row {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .chat-input-row form {
    flex: 1;
    min-width: 200px;
  }
  .chat-input-row input {
    width: 100%;
    font: inherit;
    padding: 0.5rem 0.7rem;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    color: inherit;
  }
  .dice-quick {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  .dice-quick input {
    width: 100px;
    font: inherit;
    padding: 0.5rem 0.5rem;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    color: inherit;
    font-family: ui-monospace, monospace;
  }
  .secret-chk {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    font-size: 0.75rem;
    color: var(--muted);
    cursor: pointer;
  }

  /* Initiative Panel */
  .initiative-panel {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .initiative-panel header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
  .initiative-panel h3 {
    margin: 0;
    font-size: 1rem;
  }
  .controls {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .controls button {
    font: inherit;
    font-size: 0.8rem;
    padding: 0.35rem 0.6rem;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    color: inherit;
    cursor: pointer;
  }
  .controls button.primary {
    background: color-mix(in srgb, currentColor 12%, transparent);
    border-color: color-mix(in srgb, currentColor 30%, transparent);
    font-weight: 500;
  }
  .round {
    font-size: 0.8rem;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .init-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }
  .init-list li {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.4rem 0.6rem;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
  }
  .init-list li.active {
    border-color: color-mix(in srgb, hsl(140 60% 45%) 50%, var(--line));
    background: color-mix(in srgb, hsl(140 60% 45%) 8%, transparent);
  }
  .init-list .num {
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    min-width: 1.5rem;
  }
  .init-list .name {
    flex: 1;
  }
  .move-btn {
    font-size: 0.75rem;
    padding: 0.15rem 0.35rem;
    border-radius: 4px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    color: var(--muted);
    cursor: pointer;
  }
  .move-btn:hover:not([disabled]) {
    color: inherit;
    background: color-mix(in srgb, currentColor 10%, transparent);
  }
  .move-btn[disabled] {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .add-to-init {
    display: flex;
    gap: 0.4rem;
  }
  .add-to-init select {
    flex: 1;
    font: inherit;
    padding: 0.4rem 0.5rem;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    color: inherit;
  }
  .add-to-init button {
    font: inherit;
    padding: 0.4rem 0.7rem;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    color: inherit;
    cursor: pointer;
  }

  /* Party Panel */
  .party-panel {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .party-card {
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--bg-soft);
    overflow: hidden;
  }
  .party-card .card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.5rem 0.75rem;
    background: color-mix(in srgb, var(--player-color) 15%, transparent);
    border-bottom: 1px solid var(--line);
  }
  .party-card .name {
    font-weight: 600;
  }
  .party-card .peer {
    font-size: 0.7rem;
    color: var(--muted);
    font-family: ui-monospace, monospace;
  }
  .party-card .reconnecting-badge {
    font-size: 0.65rem;
    color: #f59e0b;
    background: rgba(245, 158, 11, 0.15);
    border: 1px solid #f59e0b;
    border-radius: 4px;
    padding: 0.1rem 0.4rem;
    font-style: italic;
  }
  .sheet {
    padding: 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .row {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .row.full {
    flex-direction: column;
  }
  .row label {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
    flex: 1;
    min-width: 80px;
  }
  .row input,
  .row textarea {
    font: inherit;
    font-size: 0.85rem;
    padding: 0.4rem 0.5rem;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: color-mix(in srgb, currentColor 4%, transparent);
    color: inherit;
  }
  .row textarea {
    resize: vertical;
    min-height: 4rem;
  }
  .row input[disabled],
  .row textarea[disabled] {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .no-sheet {
    padding: 0.75rem;
    text-align: center;
    color: var(--muted);
    font-size: 0.85rem;
  }
  .party-card button.primary {
    margin: 0 0.75rem 0.75rem;
    width: calc(100% - 1.5rem);
  }

  /* Scene Panel */
  .scene-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .scene-panel header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-shrink: 0;
  }
  .scene-panel h3 {
    margin: 0;
    font-size: 1rem;
  }
  .scene-controls {
    display: flex;
    gap: 0.4rem;
  }
  .scene-controls button {
    font: inherit;
    font-size: 0.8rem;
    padding: 0.35rem 0.6rem;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    color: inherit;
    cursor: pointer;
  }
  .grid-container {
    flex: 1;
    position: relative;
    border: 1px solid var(--line);
    border-radius: 8px;
    overflow: hidden;
    background: #1a1a2e;
  }
  .grid-bg {
    position: absolute;
    inset: 0;
    background-size: cover;
    background-position: center;
    opacity: 0.4;
  }
  .grid-bg.placeholder {
    background: repeating-linear-gradient(45deg, transparent, transparent 20px, rgba(255,255,255,0.02) 20px, rgba(255,255,255,0.02) 40px);
  }
  .grid-lines {
    position: absolute;
    inset: 0;
    background-image:
      linear-gradient(rgba(255,255,255,0.05) 1px, transparent 1px),
      linear-gradient(90deg, rgba(255,255,255,0.05) 1px, transparent 1px);
    background-size: var(--cell-size) var(--cell-size);
  }
  .tokens-layer {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .token {
    position: absolute;
    width: calc(var(--cell-size) - 4px);
    height: calc(var(--cell-size) - 4px);
    margin: 2px;
    border: 2px solid;
    border-radius: 6px;
    background: color-mix(in srgb, currentColor 10%, white 90%);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    font-size: 0.65rem;
    font-weight: 600;
    pointer-events: auto;
    cursor: grab;
    user-select: none;
    box-shadow: 0 2px 4px rgba(0,0,0,0.3);
    transition: box-shadow 0.1s;
    overflow: hidden; /* clip token portraits to the rounded corner */
  }
  .token:active {
    cursor: grabbing;
    box-shadow: 0 4px 8px rgba(0,0,0,0.4);
    z-index: 10;
  }
  .token .label {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
    position: relative;
    z-index: 1;
    text-shadow: 0 1px 2px rgba(0,0,0,0.8);
  }
  /* Imagery v1 (DEDUCTION §4.3.2): NPC portrait behind the label; rendered
     only when promoted art resolves. */
  :global(.token-art) {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    border-radius: inherit;
  }
  .pc-badge {
    font-size: 0.55rem;
    padding: 0.05rem 0.2rem;
    border-radius: 999px;
    background: color-mix(in srgb, hsl(260 60% 50%) 15%, transparent);
    color: hsl(260 60% 70%);
    position: relative;
    z-index: 1;
  }
  /* Imagery v1 (DEDUCTION §4.3.2 Load flow): dismissible cover banner shown
     once per adventure load; `.adventure-badge` remains the text fallback. */
  .cover-banner {
    position: relative;
    margin: 0 0 0.75rem;
    border: 1px solid var(--line);
    border-radius: 8px;
    overflow: hidden;
  }
  .cover-banner img {
    display: block;
    width: 100%;
    max-height: 12rem;
    object-fit: cover;
  }
  .cover-banner .dismiss {
    position: absolute;
    top: 0.35rem;
    right: 0.35rem;
    width: 1.5rem;
    height: 1.5rem;
    border: none;
    border-radius: 999px;
    background: rgba(0, 0, 0, 0.55);
    color: #fff;
    font-size: 0.9rem;
    line-height: 1;
    cursor: pointer;
  }
  .cover-thumb {
    width: 2.25rem;
    height: 2.25rem;
    object-fit: cover;
    border: 1px solid var(--line);
    border-radius: 6px;
    align-self: center;
  }
  /* Imagery v1 (DEDUCTION §4.3.2): promoted handout plate above the body. */
  :global(.handout-art) {
    display: block;
    width: 100%;
    max-height: 14rem;
    object-fit: cover;
    border-bottom: 1px solid var(--line);
  }
  .scene-legend {
    display: flex;
    justify-content: space-between;
    padding: 0.5rem;
    font-size: 0.75rem;
    color: var(--muted);
    flex-shrink: 0;
  }
  .scene-legend .hint {
    font-style: italic;
  }

  /* Handouts Panel */
  .handouts-panel {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .handout-card {
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--bg-soft);
    overflow: hidden;
  }
  .handout-card header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.5rem 0.75rem;
    border-bottom: 1px solid var(--line);
  }
  .handout-card h4 {
    margin: 0;
    font-size: 0.9rem;
  }
  .handout-card button {
    font: inherit;
    font-size: 0.75rem;
    padding: 0.25rem 0.5rem;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: color-mix(in srgb, hsl(140 60% 45%) 12%, transparent);
    color: hsl(140 60% 45%);
    cursor: pointer;
  }
  .handout-body {
    padding: 0.75rem;
    min-height: 4rem;
  }

  /* GM Drawer (overlay) */
  .gm-drawer-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0,0,0,0.5);
    z-index: 100;
    display: flex;
    align-items: flex-end;
    padding-bottom: env(safe-area-inset-bottom, 0);
  }
  .gm-drawer {
    width: 100%;
    max-height: 70vh;
    background: var(--bg-soft);
    border-top-left-radius: 16px;
    border-top-right-radius: 16px;
    border: 1px solid var(--line);
    border-bottom: none;
    display: flex;
    flex-direction: column;
    box-shadow: 0 -4px 20px rgba(0,0,0,0.3);
  }
  .gm-drawer header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.75rem 1rem;
    border-bottom: 1px solid var(--line);
  }
  .gm-drawer header h3 {
    margin: 0;
    font-size: 1rem;
  }
  .gm-drawer header button {
    font-size: 1.25rem;
    line-height: 1;
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
  }
  .gm-drawer .drawer-content {
    flex: 1;
    overflow: auto;
    padding: 1rem;
  }
  .gm-drawer .drawer-chat {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    max-height: 40vh;
    overflow: auto;
  }
  .gm-drawer .msg {
    padding: 0.4rem 0.6rem;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    font-size: 0.8rem;
  }
  .gm-drawer .msg .from {
    font-size: 0.65rem;
    text-transform: uppercase;
    color: var(--muted);
    margin-right: 0.5rem;
  }
  .gm-drawer .msg .roll {
    font-family: ui-monospace, monospace;
    font-size: 0.75rem;
    color: hsl(45 90% 55%);
  }
  .gm-drawer form {
    display: flex;
    gap: 0.4rem;
    padding-top: 0.5rem;
    border-top: 1px solid var(--line);
  }
  .gm-drawer input {
    flex: 1;
    font: inherit;
    padding: 0.5rem 0.7rem;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    color: inherit;
  }

  /* GM Drawer Full (tab) */
  .gm-drawer-full {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .gm-drawer-full header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 0.5rem;
    padding-bottom: 0.5rem;
    border-bottom: 1px solid var(--line);
  }
  .gm-drawer-full h3 {
    margin: 0;
    font-size: 1rem;
  }
  .gm-actions {
    display: flex;
    gap: 0.4rem;
  }
  .gm-actions button {
    font: inherit;
    font-size: 0.8rem;
    padding: 0.35rem 0.6rem;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    color: inherit;
    cursor: pointer;
  }
  .gm-actions button.primary {
    background: color-mix(in srgb, currentColor 12%, transparent);
    border-color: color-mix(in srgb, currentColor 30%, transparent);
    font-weight: 500;
  }
  .gm-actions button.danger {
    border-color: #c33;
    color: #c33;
  }
  .drawer-tabs {
    display: flex;
    gap: 0.25rem;
    margin: 0.5rem 0;
    border-bottom: 1px solid var(--line);
    padding-bottom: 0.5rem;
  }
  .drawer-tabs button {
    flex: 1;
    font: inherit;
    font-size: 0.8rem;
    padding: 0.4rem 0.6rem;
    border-radius: 6px 6px 0 0;
    border: 1px solid var(--line);
    border-bottom: none;
    background: var(--bg-soft);
    color: var(--muted);
    cursor: pointer;
  }
  .drawer-tabs button.active {
    color: inherit;
    background: color-mix(in srgb, currentColor 8%, transparent);
    border-color: color-mix(in srgb, currentColor 30%, transparent);
  }
  .dice-tools h4 {
    margin: 0 0 0.5rem;
    font-size: 0.85rem;
  }
  .dice-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.4rem;
    margin-bottom: 0.75rem;
  }
  .dice-grid button {
    font: inherit;
    padding: 0.5rem;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    color: inherit;
    cursor: pointer;
  }
  .dice-tools form {
    display: flex;
    gap: 0.4rem;
  }
  .dice-tools input {
    flex: 1;
    font: inherit;
    font-family: ui-monospace, monospace;
    padding: 0.5rem 0.7rem;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    color: inherit;
  }
  .dice-tools button {
    font: inherit;
    padding: 0.5rem 0.8rem;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: color-mix(in srgb, hsl(45 90% 55%) 12%, transparent);
    color: hsl(45 90% 45%);
    cursor: pointer;
  }
  .gm-tools h4 {
    margin: 0 0 0.5rem;
    font-size: 0.85rem;
  }
  .gm-tools button {
    width: 100%;
    font: inherit;
    padding: 0.6rem;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    color: inherit;
    cursor: pointer;
    margin-bottom: 0.4rem;
  }
  .gm-tools p.muted {
    margin: 0 0 0.5rem;
    font-size: 0.75rem;
    color: var(--muted);
    word-break: break-all;
  }
  .gm-tools button.danger {
    width: 100%;
    font: inherit;
    padding: 0.6rem;
    border-radius: 8px;
    border: 1px solid #c33;
    background: rgba(204, 51, 51, 0.1);
    color: #c33;
    cursor: pointer;
  }
  .drawer-input {
    display: flex;
    gap: 0.4rem;
    padding-top: 0.5rem;
    border-top: 1px solid var(--line);
  }
  .drawer-input input {
    flex: 1;
    font: inherit;
    padding: 0.5rem 0.7rem;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    color: inherit;
  }

  .empty {
    text-align: center;
    color: var(--muted);
    padding: 2rem;
    font-size: 0.85rem;
  }
  .empty kbd {
    font-family: ui-monospace, monospace;
    background: var(--bg-soft);
    border: 1px solid var(--line);
    padding: 0.1rem 0.3rem;
    border-radius: 4px;
    font-size: 0.8em;
  }
</style>