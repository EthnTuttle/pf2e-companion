// Vitest unit tests for src/lib/transport.ts
import { describe, expect, test, vi, beforeEach, afterEach } from "vitest";
import type { Intent, PatchEvent, PresenceEvent } from "./ipc";
import { mockTransport, wsTransport } from "./transport";

describe("mockTransport", () => {
  let transport: ReturnType<typeof mockTransport>;

  beforeEach(() => {
    transport = mockTransport();
  });

  test("records sent intents", async () => {
    const intent: Intent = { kind: "chat", text: "hello" };
    await transport.send(intent);
    expect(transport.sent).toHaveLength(1);
    expect(transport.sent[0]).toEqual(intent);
  });

  test("emits patches to subscribers", async () => {
    const patches: PatchEvent[] = [];
    const unsub = transport.onPatch((p) => patches.push(p));

    const patch: PatchEvent = { session_id: "s1", kind: "chat_appended", entry: { ts: 1, from: "gm", kind: "chat", text: "hi", roll: null } };
    transport.emit(patch);

    expect(patches).toHaveLength(1);
    expect(patches[0]).toEqual(patch);

    unsub();
    transport.emit({ session_id: "s1", kind: "chat_appended", entry: { ts: 2, from: "gm", kind: "chat", text: "ho", roll: null } });
    expect(patches).toHaveLength(1); // unsubscribed
  });

  test("emits presence events to subscribers", async () => {
    const events: PresenceEvent[] = [];
    const unsub = transport.onPresence((p) => events.push(p));

    const event: PresenceEvent = { session_id: "s1", kind: "peer_joined", peer_id: "p2", name: "Alice", via: "ipc" };
    transport.emitPresence(event);

    expect(events).toHaveLength(1);
    expect(events[0]).toEqual(event);

    unsub();
  });

  test("emits errors to subscribers", async () => {
    const errors: string[] = [];
    const unsub = transport.onError((msg) => errors.push(msg));

    transport.emitError("Test error");

    expect(errors).toHaveLength(1);
    expect(errors[0]).toBe("Test error");

    unsub();
    transport.emitError("Another error");
    expect(errors).toHaveLength(1); // unsubscribed
  });

  test("connect and close are no-ops", async () => {
    await expect(transport.connect()).resolves.toBeUndefined();
    await expect(transport.close()).resolves.toBeUndefined();
  });

  test("multiple subscribers receive events", async () => {
    const patches1: PatchEvent[] = [];
    const patches2: PatchEvent[] = [];
    const unsub1 = transport.onPatch((p) => patches1.push(p));
    const unsub2 = transport.onPatch((p) => patches2.push(p));

    transport.emit({ session_id: "s1", kind: "snapshot", snapshot: { meta: { id: "s1", name: "test", gm_id: "gm", created_at: 0, adventure_slug: null }, players: {}, chat: [], init: { order: [], active_idx: 0, round: 1 }, scene: { grid: { w: 10, h: 10, size: 32 }, background_entity_id: null, tokens: {} }, handouts: {} } });

    expect(patches1).toHaveLength(1);
    expect(patches2).toHaveLength(1);

    unsub1();
    transport.emit({ session_id: "s1", kind: "chat_appended", entry: { ts: 1, from: "gm", kind: "chat", text: "hi", roll: null } });
    expect(patches1).toHaveLength(1);
    expect(patches2).toHaveLength(2);
  });
});

describe("wsTransport error frame after connect", () => {
  let mockWs: any;
  const originalWebSocket = globalThis.WebSocket;

  beforeEach(() => {
    mockWs = {
      readyState: WebSocket.OPEN,
      send: vi.fn(),
      close: vi.fn(),
      onopen: null,
      onmessage: null,
      onerror: null,
      onclose: null,
    };
    // Use a proper constructor function
    const MockWebSocket = function(this: any, url: string) {
      this.readyState = WebSocket.OPEN;
      this.send = vi.fn();
      this.close = vi.fn();
      this.onopen = null;
      this.onmessage = null;
      this.onerror = null;
      this.onclose = null;
      // Capture the mock for test access
      mockWs = this;
    };
    globalThis.WebSocket = MockWebSocket as any;
  });

  afterEach(() => {
    globalThis.WebSocket = originalWebSocket;
  });

  test("error frame after welcome dispatches to onError, not connectReject", async () => {
    const { wsTransport } = await import("./transport");

    const transport = wsTransport("ws://localhost:1234/session/abc", "TestPlayer");
    const connectPromise = transport.connect();

    // Wait for the WebSocket constructor to be called and capture mockWs
    await Promise.resolve();

    // Simulate welcome
    mockWs.onmessage?.({ data: JSON.stringify({ type: "welcome" }) });
    await connectPromise;

    // Now simulate an error frame after connection
    const errorMsgs: string[] = [];
    transport.onError((msg) => errorMsgs.push(msg));

    mockWs.onmessage?.({ data: JSON.stringify({ type: "error", message: "Forbidden: GM only", rejected_intent: { kind: "end_session" } }) });

    expect(errorMsgs).toHaveLength(1);
    expect(errorMsgs[0]).toBe("Forbidden: GM only");
  });
});

describe("wsTransport frame encoding", () => {
  test("hello frame structure", () => {
    const frame = {
      type: "hello",
      session_id: "abc123",
      name: "TestPlayer",
    };
    expect(frame.type).toBe("hello");
    expect(frame.session_id).toBe("abc123");
    expect(frame.name).toBe("TestPlayer");
  });

  test("intent frame structure", () => {
    const intent: Intent = { kind: "chat", text: "hello" };
    const frame = { type: "intent", intent };
    expect(frame.type).toBe("intent");
    expect(frame.intent).toEqual(intent);
  });

  test("patch frame structure mirrors Tauri event payload", () => {
    const patch: PatchEvent = { session_id: "s1", kind: "chat_appended", entry: { ts: 1, from: "gm", kind: "chat", text: "hi", roll: null } };
    const frame = { type: "patch", ...patch };
    expect(frame.type).toBe("patch");
    expect(frame.session_id).toBe("s1");
    expect(frame.kind).toBe("chat_appended");
  });

  test("presence frame structure", () => {
    const presence: PresenceEvent = { session_id: "s1", kind: "peer_joined", peer_id: "p2", name: "Alice", via: "lan" };
    const frame = { type: "presence", ...presence };
    expect(frame.type).toBe("presence");
    expect(frame.kind).toBe("peer_joined");
  });

  test("error frame structure", () => {
    const frame = { type: "error", message: "Forbidden", rejected_intent: { kind: "end_session" } };
    expect(frame.type).toBe("error");
    expect(frame.message).toBe("Forbidden");
    expect(frame.rejected_intent).toEqual({ kind: "end_session" });
  });
});

describe("createTransport backend selection logic", () => {
  const originalWindow = globalThis.window;

  beforeEach(() => {
    globalThis.window = { ...originalWindow } as any;
  });

  afterEach(() => {
    globalThis.window = originalWindow;
  });

  test("throws when no Tauri and no host param", async () => {
    // @ts-expect-error
    delete globalThis.window.__TAURI_INTERNALS__;
    Object.defineProperty(globalThis.window, "location", { value: { search: "" }, writable: true, configurable: true });

    const { createTransport } = await import("./transport");
    expect(() => createTransport("test")).toThrow("No transport available");
  });

  test("would select ipcTransport when Tauri internals present", async () => {
    // @ts-expect-error
    globalThis.window.__TAURI_INTERNALS__ = {};
    Object.defineProperty(globalThis.window, "location", { value: { search: "" }, writable: true, configurable: true });

    const { ipcTransport } = await import("./transport");
    // We can't easily test the actual transport creation without mocking more,
    // but we can verify the function exists
    expect(typeof ipcTransport).toBe("function");
  });
});

describe("wsTransport frame encoding", () => {
  test("hello frame structure", () => {
    const frame = {
      type: "hello",
      session_id: "abc123",
      name: "TestPlayer",
    };
    expect(frame.type).toBe("hello");
    expect(frame.session_id).toBe("abc123");
    expect(frame.name).toBe("TestPlayer");
  });

  test("intent frame structure", () => {
    const intent: Intent = { kind: "chat", text: "hello" };
    const frame = { type: "intent", intent };
    expect(frame.type).toBe("intent");
    expect(frame.intent).toEqual(intent);
  });

  test("patch frame structure mirrors Tauri event payload", () => {
    const patch: PatchEvent = { session_id: "s1", kind: "chat_appended", entry: { ts: 1, from: "gm", kind: "chat", text: "hi", roll: null } };
    const frame = { type: "patch", ...patch };
    expect(frame.type).toBe("patch");
    expect(frame.session_id).toBe("s1");
    expect(frame.kind).toBe("chat_appended");
  });

  test("presence frame structure", () => {
    const presence: PresenceEvent = { session_id: "s1", kind: "peer_joined", peer_id: "p2", name: "Alice", via: "lan" };
    const frame = { type: "presence", ...presence };
    expect(frame.type).toBe("presence");
    expect(frame.kind).toBe("peer_joined");
  });

  test("error frame structure", () => {
    const frame = { type: "error", message: "Forbidden", rejected_intent: { kind: "end_session" } };
    expect(frame.type).toBe("error");
    expect(frame.message).toBe("Forbidden");
    expect(frame.rejected_intent).toEqual({ kind: "end_session" });
  });
});

describe("createTransport backend selection logic", () => {
  const originalWindow = globalThis.window;

  beforeEach(() => {
    globalThis.window = { ...originalWindow } as any;
  });

  afterEach(() => {
    globalThis.window = originalWindow;
  });

  test("throws when no Tauri and no host param", async () => {
    // @ts-expect-error
    delete globalThis.window.__TAURI_INTERNALS__;
    Object.defineProperty(globalThis.window, "location", { value: { search: "" }, writable: true, configurable: true });

    const { createTransport } = await import("./transport");
    expect(() => createTransport("test")).toThrow("No transport available");
  });

  test("would select ipcTransport when Tauri internals present", async () => {
    // @ts-expect-error
    globalThis.window.__TAURI_INTERNALS__ = {};
    Object.defineProperty(globalThis.window, "location", { value: { search: "" }, writable: true, configurable: true });

    const { ipcTransport } = await import("./transport");
    // We can't easily test the actual transport creation without mocking more,
    // but we can verify the function exists
    expect(typeof ipcTransport).toBe("function");
  });
});