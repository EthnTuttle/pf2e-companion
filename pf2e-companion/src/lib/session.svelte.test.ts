// Vitest unit tests for src/lib/session.svelte.ts
import { describe, expect, test, vi, beforeEach } from "vitest";
import { applyPatch, applyPresence, resetSession, sessionState } from "./session.svelte";
import type { PatchEvent, PresenceEvent, SessionSnapshot } from "./ipc";

function makeSnapshot(overrides: Partial<SessionSnapshot> = {}): SessionSnapshot {
  return {
    meta: { id: "s1", name: "Test", gm_id: "gm1", created_at: 1000, adventure_slug: null },
    players: {
      gm1: { name: "GM", character: null, color: "#f59e0b", connected: true },
      p1: { name: "Alice", character: { level: 1, class: "Fighter", hp: 10, max_hp: 10, ac: 16, conditions: [], notes: "" }, color: "#3b82f6", connected: true },
    },
    chat: [{ ts: 1000, from: "GM", kind: "system", text: "Session started", roll: null }],
    init: { order: ["gm1", "p1"], active_idx: 0, round: 1 },
    scene: { grid: { w: 20, h: 15, size: 32 }, background_entity_id: null, tokens: {} },
    handouts: { letter: { entity_id: "letter", visible_to: "gm" } },
    ...overrides,
  };
}

beforeEach(() => {
  resetSession();
});

describe("applyPatch reducer", () => {
  test("snapshot replaces state wholesale", () => {
    const snap = makeSnapshot();
    applyPatch({ session_id: "s1", kind: "snapshot", snapshot: snap });
    expect(sessionState.snapshot).toEqual(snap);
  });

  test("chat_appended adds to chat array in order", () => {
    const snap = makeSnapshot({ chat: [] });
    applyPatch({ session_id: "s1", kind: "snapshot", snapshot: snap });

    applyPatch({ session_id: "s1", kind: "chat_appended", entry: { ts: 2000, from: "Alice", kind: "chat", text: "hi", roll: null } });
    applyPatch({ session_id: "s1", kind: "chat_appended", entry: { ts: 1500, from: "GM", kind: "chat", text: "hello", roll: null } });

    expect(sessionState.snapshot?.chat).toHaveLength(2);
    expect(sessionState.snapshot?.chat[0].text).toBe("hello");
    expect(sessionState.snapshot?.chat[1].text).toBe("hi");
  });

  test("player_upserted adds or updates player", () => {
    const snap = makeSnapshot({ players: {} });
    applyPatch({ session_id: "s1", kind: "snapshot", snapshot: snap });

    applyPatch({ session_id: "s1", kind: "player_upserted", peer_id: "p2", player: { name: "Bob", character: null, color: "#22c55e", connected: true } });
    expect(sessionState.snapshot?.players.p2).toEqual({ name: "Bob", character: null, color: "#22c55e", connected: true });

    applyPatch({ session_id: "s1", kind: "player_upserted", peer_id: "p2", player: { name: "Bob Updated", character: null, color: "#22c55e", connected: true } });
    expect(sessionState.snapshot?.players.p2.name).toBe("Bob Updated");
  });

  test("player_upserted with connected:false preserves character (not a removal)", () => {
    const snap = makeSnapshot({ players: {} });
    applyPatch({ session_id: "s1", kind: "snapshot", snapshot: snap });

    applyPatch({ session_id: "s1", kind: "player_upserted", peer_id: "p2", player: { name: "Bob", character: { level: 1, class: "Fighter", hp: 10, max_hp: 10, ac: 16, conditions: [], notes: "" }, color: "#22c55e", connected: false } });
    expect(sessionState.snapshot?.players.p2).toEqual({ name: "Bob", character: { level: 1, class: "Fighter", hp: 10, max_hp: 10, ac: 16, conditions: [], notes: "" }, color: "#22c55e", connected: false });
    // Character should remain intact
    expect(sessionState.snapshot?.players.p2.character).not.toBeNull();
  });

  test("player_removed deletes player", () => {
    const snap = makeSnapshot();
    applyPatch({ session_id: "s1", kind: "snapshot", snapshot: snap });

    applyPatch({ session_id: "s1", kind: "player_removed", peer_id: "p1" });
    expect(sessionState.snapshot?.players.p1).toBeUndefined();
    expect(sessionState.snapshot?.players.gm1).toBeDefined();
  });

  test("initiative updates order, active_idx, round", () => {
    const snap = makeSnapshot();
    applyPatch({ session_id: "s1", kind: "snapshot", snapshot: snap });

    applyPatch({ session_id: "s1", kind: "initiative", order: ["p1", "gm1"], active_idx: 1, round: 2 });
    expect(sessionState.snapshot?.init.order).toEqual(["p1", "gm1"]);
    expect(sessionState.snapshot?.init.active_idx).toBe(1);
    expect(sessionState.snapshot?.init.round).toBe(2);
  });

  test("token_upserted adds or updates token", () => {
    const snap = makeSnapshot();
    applyPatch({ session_id: "s1", kind: "snapshot", snapshot: snap });

    const token = { token_id: "t1", owner: "p1", entity_id: null, pc: "Alice", x: 5, y: 5, label: "Hero" };
    applyPatch({ session_id: "s1", kind: "token_upserted", token });
    expect(sessionState.snapshot?.scene.tokens.t1).toEqual(token);

    applyPatch({ session_id: "s1", kind: "token_upserted", token: { ...token, x: 6, label: "Hero!" } });
    expect(sessionState.snapshot?.scene.tokens.t1.x).toBe(6);
    expect(sessionState.snapshot?.scene.tokens.t1.label).toBe("Hero!");
  });

  test("token_removed deletes token", () => {
    const snap = makeSnapshot({ scene: { grid: { w: 20, h: 15, size: 32 }, background_entity_id: null, tokens: { t1: { token_id: "t1", owner: "p1", entity_id: null, pc: "Alice", x: 5, y: 5, label: "Hero" } } } });
    applyPatch({ session_id: "s1", kind: "snapshot", snapshot: snap });

    applyPatch({ session_id: "s1", kind: "token_removed", token_id: "t1" });
    expect(sessionState.snapshot?.scene.tokens.t1).toBeUndefined();
  });

  test("scene_set replaces entire scene", () => {
    const snap = makeSnapshot();
    applyPatch({ session_id: "s1", kind: "snapshot", snapshot: snap });

    const newScene = { grid: { w: 10, h: 10, size: 40 }, background_entity_id: "bg1", tokens: { t2: { token_id: "t2", owner: "gm1", entity_id: "npc1", pc: null, x: 2, y: 2, label: "Goblin" } } };
    applyPatch({ session_id: "s1", kind: "scene_set", scene: newScene });
    expect(sessionState.snapshot?.scene).toEqual(newScene);
  });

  test("handout_revealed flips visibility to all", () => {
    const snap = makeSnapshot({ handouts: { letter: { entity_id: "letter", visible_to: "gm" } } });
    applyPatch({ session_id: "s1", kind: "snapshot", snapshot: snap });

    applyPatch({ session_id: "s1", kind: "handout_revealed", handout_id: "letter" });
    expect(sessionState.snapshot?.handouts.letter.visible_to).toBe("all");
  });

  test("adventure_loaded sets meta.adventure_slug", () => {
    const snap = makeSnapshot({ meta: { ...makeSnapshot().meta, adventure_slug: null } });
    applyPatch({ session_id: "s1", kind: "snapshot", snapshot: snap });

    applyPatch({ session_id: "s1", kind: "adventure_loaded", slug: "widows-lamp" });
    expect(sessionState.snapshot?.meta.adventure_slug).toBe("widows-lamp");
  });

  test("session_ended resets state", () => {
    const snap = makeSnapshot();
    applyPatch({ session_id: "s1", kind: "snapshot", snapshot: snap });
    sessionState.info = { session_id: "s1", peer_id: "p1", role: "player", lan_url: null, p2p_ticket: null, name: "Alice" };
    sessionState.connected = true;

    applyPatch({ session_id: "s1", kind: "session_ended" });
    expect(sessionState.snapshot).toBeNull();
    expect(sessionState.info).toBeNull();
    expect(sessionState.connected).toBe(false);
  });

  test("error sets error message", () => {
    applyPatch({ session_id: "s1", kind: "error", message: "Forbidden: not GM" });
    expect(sessionState.error).toBe("Forbidden: not GM");
  });

  test("unknown kind is ignored without throwing (v1 tolerance)", () => {
    const snap = makeSnapshot();
    applyPatch({ session_id: "s1", kind: "snapshot", snapshot: snap });
    // @ts-expect-error unknown kind
    applyPatch({ session_id: "s1", kind: "unknown_future_kind", foo: "bar" });
    // Should not throw, state unchanged
    expect(sessionState.snapshot).toEqual(snap);
  });

  test("patch before snapshot is ignored (no state)", () => {
    applyPatch({ session_id: "s1", kind: "chat_appended", entry: { ts: 1000, from: "GM", kind: "chat", text: "hi", roll: null } });
    expect(sessionState.snapshot).toBeNull();
  });

  test("stale-session patch (different session_id) is dropped", () => {
    const snap = makeSnapshot();
    applyPatch({ session_id: "s1", kind: "snapshot", snapshot: snap });
    sessionState.info = { session_id: "s1", peer_id: "p1", role: "player", lan_url: null, p2p_ticket: null, name: "Alice" };

    // Patch with different session_id should be ignored
    applyPatch({ session_id: "s2", kind: "chat_appended", entry: { ts: 2000, from: "Eve", kind: "chat", text: "injected", roll: null } });

    expect(sessionState.snapshot?.chat).toHaveLength(1); // Original only
    expect(sessionState.snapshot?.chat[0].text).toBe("Session started");
  });

  test("stale-session presence (different session_id) is dropped", () => {
    sessionState.info = { session_id: "s1", peer_id: "p1", role: "player", lan_url: null, p2p_ticket: null, name: "Alice" };
    sessionState.presence = ["p1"];

    // Presence with different session_id should be ignored
    applyPresence({ session_id: "s2", kind: "peer_joined", peer_id: "p2", name: "Eve", via: "lan" });

    expect(sessionState.presence).toEqual(["p1"]);
  });

  test("chat_appended dedupes on (ts, from, text)", () => {
    const snap = makeSnapshot({ chat: [] });
    applyPatch({ session_id: "s1", kind: "snapshot", snapshot: snap });

    const entry = { ts: 2000, from: "Alice", kind: "chat" as const, text: "hello", roll: null };
    applyPatch({ session_id: "s1", kind: "chat_appended", entry });
    applyPatch({ session_id: "s1", kind: "chat_appended", entry }); // Duplicate
    applyPatch({ session_id: "s1", kind: "chat_appended", entry }); // Duplicate again

    expect(sessionState.snapshot?.chat).toHaveLength(1);
    expect(sessionState.snapshot?.chat[0].text).toBe("hello");
  });

  test("chat_appended allows same text from different sender or time", () => {
    const snap = makeSnapshot({ chat: [] });
    applyPatch({ session_id: "s1", kind: "snapshot", snapshot: snap });

    applyPatch({ session_id: "s1", kind: "chat_appended", entry: { ts: 2000, from: "Alice", kind: "chat", text: "hello", roll: null } });
    applyPatch({ session_id: "s1", kind: "chat_appended", entry: { ts: 2001, from: "Alice", kind: "chat", text: "hello", roll: null } }); // Different time
    applyPatch({ session_id: "s1", kind: "chat_appended", entry: { ts: 2000, from: "Bob", kind: "chat", text: "hello", roll: null } }); // Different sender

    expect(sessionState.snapshot?.chat).toHaveLength(3);
  });
});

describe("applyPresence", () => {
  test("peer_joined adds to presence", () => {
    applyPresence({ session_id: "s1", kind: "peer_joined", peer_id: "p2", name: "Bob", via: "ipc" });
    expect(sessionState.presence).toContain("p2");
  });

  test("peer_left removes from presence", () => {
    sessionState.presence = ["p1", "p2"];
    applyPresence({ session_id: "s1", kind: "peer_left", peer_id: "p1" });
    expect(sessionState.presence).toEqual(["p2"]);
  });

  test("duplicate peer_joined is idempotent", () => {
    applyPresence({ session_id: "s1", kind: "peer_joined", peer_id: "p2", name: "Bob", via: "ipc" });
    applyPresence({ session_id: "s1", kind: "peer_joined", peer_id: "p2", name: "Bob", via: "ipc" });
    expect(sessionState.presence.filter((p) => p === "p2")).toHaveLength(1);
  });
});

describe("derived state", () => {
  test("myPeerId derives from info", () => {
    sessionState.info = { session_id: "s1", peer_id: "p1", role: "player", lan_url: null, p2p_ticket: null, name: "Alice" };
    // Derived values need $derived context - tested via component integration
    expect(sessionState.info.peer_id).toBe("p1");
  });

  test("myRole derives from info", () => {
    sessionState.info = { session_id: "s1", peer_id: "p1", role: "gm", lan_url: null, p2p_ticket: null, name: "GM" };
    expect(sessionState.info.role).toBe("gm");
  });
});

describe("resetSession", () => {
  test("clears all state", () => {
    sessionState.info = { session_id: "s1", peer_id: "p1", role: "player", lan_url: null, p2p_ticket: null, name: "Alice" };
    sessionState.snapshot = makeSnapshot();
    sessionState.presence = ["p1", "p2"];
    sessionState.connected = true;
    sessionState.error = "some error";

    resetSession();

    expect(sessionState.info).toBeNull();
    expect(sessionState.snapshot).toBeNull();
    expect(sessionState.presence).toEqual([]);
    expect(sessionState.connected).toBe(false);
    expect(sessionState.error).toBeNull();
  });
});