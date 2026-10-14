// Shared session state. Svelte 5 runes; mirrors llm.svelte.ts / lens.svelte.ts shape.
import type { SessionInfo, SessionSnapshot, PatchEvent, PresenceEvent, Token, Grid, ChatEntry } from "$lib/ipc";

export const sessionState = $state({
  info: null as SessionInfo | null,
  snapshot: null as SessionSnapshot | null,
  presence: [] as string[],
  connected: false,
  error: null as string | null,
  connectionState: "offline" as "offline" | "connecting" | "connected" | "reconnecting" | "failed",
  reconnectAttempt: 0,
  transport: null as { retryConnection: () => Promise<void>; close: () => Promise<void> } | null,
});

/** Apply a single PatchEvent to the session snapshot (reducer). */
export function applyPatch(patch: PatchEvent): void {
  // Drop patches from other sessions (stale-session protection)
  if (sessionState.info && patch.session_id !== sessionState.info.session_id) {
    return;
  }

  // Handle patches that work without a snapshot first
  switch (patch.kind) {
    case "error":
      sessionState.error = patch.message;
      return;
    case "session_ended":
      sessionState.snapshot = null;
      sessionState.info = null;
      sessionState.connected = false;
      sessionState.connectionState = "offline";
      return;
  }

  if (!sessionState.snapshot) {
    if (patch.kind === "snapshot") {
      sessionState.snapshot = patch.snapshot;
    }
    return;
  }

  const snap = sessionState.snapshot;

  switch (patch.kind) {
    case "snapshot":
      sessionState.snapshot = patch.snapshot;
      break;
    case "chat_appended":
      // Dedupe: prefer (seq, from) when seq present, fallback to legacy (ts,from,text)
      const entry = patch.entry;
      const dup = entry.seq != null
        ? snap.chat.some((e) => e.seq === entry.seq && e.from === entry.from)
        : snap.chat.some((e) => e.seq == null && e.ts === entry.ts && e.from === entry.from && e.text === entry.text);
      if (!dup) {
        snap.chat.push(entry);
        snap.chat.sort((a, b) => (a.seq ?? a.ts) - (b.seq ?? b.ts));
      }
      break;
    case "player_upserted":
      snap.players[patch.peer_id] = patch.player;
      break;
    case "player_removed":
      delete snap.players[patch.peer_id];
      break;
    case "initiative":
      snap.init.order = patch.order;
      snap.init.active_idx = patch.active_idx;
      snap.init.round = patch.round;
      break;
    case "token_upserted":
      snap.scene.tokens[patch.token.token_id] = patch.token;
      break;
    case "token_removed":
      delete snap.scene.tokens[patch.token_id];
      break;
    case "scene_set":
      snap.scene = patch.scene;
      break;
    case "handout_revealed":
      if (snap.handouts[patch.handout_id]) {
        snap.handouts[patch.handout_id].visible_to = "all";
      }
      break;
    case "adventure_loaded":
      snap.meta.adventure_slug = patch.slug;
      break;
    default:
      // v1 tolerance: ignore unknown kinds
      break;
  }
}

/** Apply a presence event. */
export function applyPresence(event: PresenceEvent): void {
  // Drop presence events from other sessions
  if (sessionState.info && event.session_id !== sessionState.info.session_id) {
    return;
  }
  if (event.kind === "peer_joined") {
    if (!sessionState.presence.includes(event.peer_id)) {
      sessionState.presence.push(event.peer_id);
    }
  } else if (event.kind === "peer_left") {
    sessionState.presence = sessionState.presence.filter((p) => p !== event.peer_id);
  }
}

/** Send an intent through the transport. */
export function createSendIntent(transport: { send: (intent: unknown) => Promise<void> }) {
  return async (intent: unknown) => {
    sessionState.error = null;
    await transport.send(intent);
  };
}

/** Set the transport for reconnection handling. */
export function setTransport(transport: { retryConnection: () => Promise<void>; close: () => Promise<void> } | null) {
  sessionState.transport = transport;
}

/** Update connection state from transport. */
export function setConnectionState(state: "offline" | "connecting" | "connected" | "reconnecting" | "failed", attempt = 0) {
  sessionState.connectionState = state;
  sessionState.reconnectAttempt = attempt;
  sessionState.connected = state === "connected";
}

/** Retry connection after failure. */
export async function retryConnection(): Promise<void> {
  sessionState.error = null;
  if (sessionState.transport) {
    await sessionState.transport.retryConnection();
  }
}

/** Derived: my peer_id from info. */
export function getMyPeerId() {
  return sessionState.info?.peer_id ?? null;
}

/** Derived: my role from info. */
export function getMyRole() {
  return sessionState.info?.role ?? null;
}

/** Derived: am I the GM? */
export function getIsGm() {
  return getMyRole() === "gm";
}

/** Derived: my player entry from snapshot.players. */
export function getMyPlayer() {
  const peerId = getMyPeerId();
  if (!sessionState.snapshot || !peerId) return null;
  return sessionState.snapshot.players[peerId] ?? null;
}

/** Derived: active initiative token peer_id. */
export function getActiveInitiativePeer() {
  if (!sessionState.snapshot) return null;
  const { order, active_idx } = sessionState.snapshot.init;
  return order[active_idx] ?? null;
}

/** Derived: chat entries sorted by timestamp (already append-only). */
export function getChatEntries(): ChatEntry[] {
  return sessionState.snapshot?.chat ?? [];
}

/** Derived: tokens array for rendering. */
export function getTokens(): Token[] {
  if (!sessionState.snapshot) return [];
  return Object.values(sessionState.snapshot.scene.tokens);
}

/** Derived: handouts visible to me (all if player, all+gm if GM). */
export function getVisibleHandouts(): Array<{ id: string; entity_id: string; visible_to: "all" | "gm" }> {
  if (!sessionState.snapshot) return [];
  return Object.entries(sessionState.snapshot.handouts)
    .filter(([, h]) => h.visible_to === "all" || getIsGm())
    .map(([id, h]) => ({ id, ...h }));
}

/** Derived: grid config. */
export function getGrid(): Grid {
  return sessionState.snapshot?.scene.grid ?? { w: 20, h: 15, size: 32 };
}

/** Reset session state (on leave/disconnect). */
export function resetSession(): void {
  sessionState.info = null;
  sessionState.snapshot = null;
  sessionState.presence = [];
  sessionState.connected = false;
  sessionState.error = null;
  sessionState.connectionState = "offline";
  sessionState.reconnectAttempt = 0;
  sessionState.transport = null;
}

/** Clear the current error (for dismissible banner). */
export function clearError(): void {
  sessionState.error = null;
}