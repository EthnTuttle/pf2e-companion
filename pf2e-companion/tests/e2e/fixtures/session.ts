// Fixtures for the live-table surface (PLAN §3.3, §4 Groups D–F land in 12.2).
//
// Fixed session/peer ids: applyPatch/applyPresence drop events whose
// session_id doesn't match sessionState.info (session.svelte.ts stale-session
// guard), so the scripted patch stream must carry "sess-e2e" throughout. The
// test script plays the authority layer (§3.4) — these fixtures are what the
// real server WOULD send, not what it must send.
import type {
  ChatEntry,
  RollRecord,
  SessionInfo,
  SessionSnapshot,
} from "$lib/ipc";

export const fixtureSessionId = "sess-e2e";
export const fixtureGmPeerId = "gm-peer-01";
export const fixturePlayerPeerId = "pl-peer-02";
export const fixtureGmName = "GM Dana";
export const fixturePlayerName = "Alice";
export const fixtureSessionName = "Widow's Lamp — Session 1";
/** PIN / URLs per PLAN §3.3 — pinned so lobby invite evidence is stable. */
export const fixtureSessionPin = "A7K9M2";
export const fixtureLanUrl = "ws://192.168.1.42:4780/abc123";
export const fixtureP2pTicket = "pf2e-ticket-e2e-a7k9m2";

/** What the harness returns for session_host (S14 stubs, then hostViaLobby). */
export const fixtureSessionInfoGm: SessionInfo = {
  session_id: fixtureSessionId,
  peer_id: fixtureGmPeerId,
  role: "gm",
  lan_url: fixtureLanUrl,
  p2p_ticket: fixtureP2pTicket,
  name: fixtureSessionName,
  pin: fixtureSessionPin,
};

/** What the harness returns for session_join_peer (joinViaLobby stubs). */
export const fixtureSessionInfoPlayer: SessionInfo = {
  session_id: fixtureSessionId,
  peer_id: fixturePlayerPeerId,
  role: "player",
  lan_url: fixtureLanUrl,
  p2p_ticket: fixtureP2pTicket,
  name: fixtureSessionName,
  pin: fixtureSessionPin,
};

type PlayerEntry = SessionSnapshot["players"][string];

export const fixtureGmPlayer: PlayerEntry = {
  name: fixtureGmName,
  character: null,
  color: "#f59e0b",
  connected: true,
};

export const fixtureAlicePlayer: PlayerEntry = {
  name: fixturePlayerName,
  character: null,
  color: "#3b82f6",
  connected: true,
};

/**
 * The roster the real host carries before anyone joins — service.rs host()
 * peer_joined()s the GM into the doc at session start, so a realistic
 * GM-only snapshot has exactly one player entry.
 */
export const fixturePlayersGmOnly: SessionSnapshot["players"] = {
  [fixtureGmPeerId]: fixtureGmPlayer,
};

/** GM + freshly joined Alice (the S17+ working roster). */
export const fixturePlayersGmAndAlice: SessionSnapshot["players"] = {
  [fixtureGmPeerId]: fixtureGmPlayer,
  [fixturePlayerPeerId]: fixtureAlicePlayer,
};

/**
 * Monotonic chat timestamps. The chat log's {#each} keys on entry.ts
 * (table/play/+page.svelte), so duplicate ts across rendered entries breaks
 * rendering — builders cannot hand-write ts (PLAN §8 risk 3).
 */
let ts = 1_700_000_000_000;
export const nextTs = (): number => ++ts;

/** A chat/roll entry with a guaranteed-unique timestamp. */
export function chatEntry(e: {
  from: string;
  kind: ChatEntry["kind"];
  text: string;
  roll?: RollRecord | null;
}): ChatEntry {
  return {
    ts: nextTs(),
    from: e.from,
    kind: e.kind,
    text: e.text,
    roll: e.roll ?? null,
  };
}

/** S18 scene content: one GM token on a small grid (legend string asserted). */
export const fixtureScene: SessionSnapshot["scene"] = {
  grid: { w: 10, h: 8, size: 32 },
  background_entity_id: null,
  tokens: {
    "tok-widow-01": {
      token_id: "tok-widow-01",
      owner: fixtureGmPeerId,
      entity_id: null,
      pc: null,
      x: 3,
      y: 2,
      label: "Widow",
    },
  },
};

/** The hidden handout S18/S21 revolve around ("letter" id per §4). */
export const fixtureHandoutsGmOnly: SessionSnapshot["handouts"] = {
  letter: { entity_id: "millbrook:note-widows-lamp", visible_to: "gm" },
};

export interface SnapshotOverrides {
  name?: string;
  adventureSlug?: string | null;
  players?: SessionSnapshot["players"];
  chat?: SessionSnapshot["chat"];
  init?: SessionSnapshot["init"];
  scene?: SessionSnapshot["scene"];
  handouts?: SessionSnapshot["handouts"];
}

/**
 * Build a GM-authority snapshot for emission. Defaults describe a brand-new
 * session: no players (roster is an explicit opt-in — see fixturePlayers*),
 * empty chat, empty initiative, default grid, no handouts.
 */
export function gmSnapshotBuilder(o: SnapshotOverrides = {}): SessionSnapshot {
  return {
    meta: {
      id: fixtureSessionId,
      name: o.name ?? fixtureSessionName,
      gm_id: fixtureGmPeerId,
      created_at: 1_700_000_000_000,
      adventure_slug: o.adventureSlug ?? null,
    },
    players: o.players ?? {},
    chat: o.chat ?? [],
    init: o.init ?? { order: [], active_idx: 0, round: 0 },
    scene: o.scene ?? {
      grid: { w: 20, h: 15, size: 32 },
      background_entity_id: null,
      tokens: {},
    },
    handouts: o.handouts ?? {},
  };
}
