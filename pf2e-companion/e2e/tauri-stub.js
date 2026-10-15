// e2e/tauri-stub.js — the minimal Tauri-IPC-in-a-browser surface (init script).
//
// Injected via context.addInitScript BEFORE any page script, so the SvelteKit
// bundle boots against `window.__TAURI_INTERNALS__` exactly as it does in the
// desktop shell. Fixture payloads come from `window.__E2E_FIXTURES__`, a plain
// JSON map installed by a preceding init script. Unknown commands throw loudly
// (`e2e: unknown command <cmd>`) — never a silent blank page.
//
// Mirrors @tauri-apps/api/mocks.js: every invoke() from @tauri-apps/api/core
// delegates to window.__TAURI_INTERNALS__.invoke, listen() is just
// `plugin:event|listen` + transformCallback, and unlisten touches
// __TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener synchronously.
(() => {
  /** @type {Map<number, { cb: (payload: unknown) => void, once: boolean }>} */
  const callbacks = new Map();
  let nextCallbackId = 1;

  /** @type {Map<string, Set<number>>} event name -> registered callback ids */
  const listeners = new Map();

  /** Scripted LLM replay: set by llm_chat, flushed on first llm:token listen. */
  /** @type {Array<Record<string, unknown>> | null} */
  let pendingReplay = null;

  /**
   * @param {(payload: unknown) => void} cb
   * @param {boolean} [once]
   * @returns {number}
   */
  function transformCallback(cb, once = false) {
    const id = nextCallbackId++;
    callbacks.set(id, { cb, once: !!once });
    return id;
  }

  /** @param {number} id @param {unknown} payload */
  function runCallback(id, payload) {
    const entry = callbacks.get(id);
    if (!entry) return;
    if (entry.once) callbacks.delete(id);
    entry.cb(payload);
  }

  /** @param {number} id */
  function unregisterCallback(id) {
    callbacks.delete(id);
  }

  /** @param {string} event @param {unknown} payload */
  function emit(event, payload) {
    const ids = listeners.get(event);
    if (!ids) return;
    for (const id of [...ids]) {
      runCallback(id, { event, id, payload });
    }
  }

  // The fixed scripted chat reply (stubbed backend, labeled as such in the
  // manifest). Assembled from 6 token fragments + 1 tool pair + done.
  const CHAT_SESSION = "e2e-chat-1";
  const chatEvents = [
    { session_id: CHAT_SESSION, kind: "token", token: "Based on the Lewisian lens, " },
    { session_id: CHAT_SESSION, kind: "token", token: "the aboleth's lure of the deep traces back " },
    { session_id: CHAT_SESSION, kind: "token", token: "to the Watchers' forbidden-arts catalogue (1 Enoch). " },
    {
      session_id: CHAT_SESSION,
      kind: "tool_start",
      id: "tool-1",
      name: "search",
      input: { query: "aboleth", lens: "lewisian" },
    },
    {
      session_id: CHAT_SESSION,
      kind: "tool_result",
      id: "tool-1",
      name: "search",
      result: { hits: [{ id: "e2e-aboleth", title: "Aboleth", type: "creature", score: 0.91 }] },
      error: false,
    },
    { session_id: CHAT_SESSION, kind: "token", token: "For four 5th-level heroes a single aboleth is severe — " },
    { session_id: CHAT_SESSION, kind: "token", token: "budget 120 XP, " },
    { session_id: CHAT_SESSION, kind: "token", token: "and its enslavement aura punishes diving alone." },
    {
      session_id: CHAT_SESSION,
      kind: "done",
      input_tokens: 412,
      output_tokens: 198,
      cache_read_input_tokens: null,
      cache_creation_input_tokens: null,
      iterations: 2,
    },
  ];

  /**
   * Resolve a fixture entry. Arg-sensitive commands use `{ __by: "<argName>",
   * "<argValue>": payload }` maps; everything else is returned as-is. Every
   * result is structuredClone'd per call so one page's mutations can't leak.
   * @param {string} cmd
   * @param {Record<string, unknown>} args
   * @returns {unknown}
   */
  function dispatch(cmd, args) {
    const fixtures = /** @type {Record<string, unknown>} */ (
      /** @type {any} */ (window).__E2E_FIXTURES__ ?? {}
    );
    const fixture = fixtures[cmd];
    if (fixture === undefined) {
      throw new Error(`e2e: unknown command ${cmd}`);
    }
    if (
      fixture !== null &&
      typeof fixture === "object" &&
      !Array.isArray(fixture) &&
      typeof (/** @type {any} */ (fixture).__by) === "string"
    ) {
      const by = /** @type {any} */ (fixture).__by;
      const key = String(args[by]);
      const entry = /** @type {any} */ (fixture)[key];
      if (entry === undefined) {
        throw new Error(`e2e: no ${cmd} fixture for ${by}=${key}`);
      }
      return structuredClone(entry);
    }
    return structuredClone(fixture);
  }

  /**
   * @param {string} cmd
   * @param {Record<string, unknown>} [args]
   * @returns {Promise<unknown>}
   */
  async function invoke(cmd, args = {}) {
    switch (cmd) {
      case "plugin:event|listen": {
        const event = String(args.event);
        const handlerId = Number(args.handler);
        if (!listeners.has(event)) listeners.set(event, new Set());
        /** @type {Set<number>} */ (listeners.get(event)).add(handlerId);
        // Deterministic hand-off: the chat page subscribes to llm:token only
        // AFTER llm_chat resolves, so the replay flushes here — first listen.
        if (event === "llm:token" && pendingReplay) {
          const events = pendingReplay;
          pendingReplay = null;
          events.forEach((ev, i) => {
            setTimeout(() => emit("llm:token", ev), 30 * (i + 1));
          });
        }
        return handlerId;
      }
      case "plugin:event|unlisten": {
        const ids = listeners.get(String(args.event));
        if (ids) ids.delete(Number(args.eventId));
        return null;
      }
      case "llm_chat": {
        // Real contract: llm_chat resolves to the session id immediately;
        // tokens stream later over llm:token. Stub the backend, replay fixed
        // events through the page's real listen path.
        pendingReplay = structuredClone(chatEvents);
        return CHAT_SESSION;
      }
      case "wizard_validate_vault": {
        // Echo the path minus any trailing "/" (the normalized form the
        // wizard stores) — arg-sensitive, so it lives here, not in fixtures.
        return String(args.path).replace(/\/+$/, "");
      }
      default:
        return dispatch(cmd, args);
    }
  }

  /** @type {any} */ (window).__TAURI_INTERNALS__ = {
    invoke,
    transformCallback,
    unregisterCallback,
  };
  /** @type {any} */ (window).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    /** @param {string} event @param {number} eventId */
    unregisterListener(event, eventId) {
      const ids = listeners.get(event);
      if (ids) ids.delete(eventId);
    },
  };
  /** Test hook: replay an event into every registered listener. */
  /** @type {any} */ (window).__e2eEmit = emit;
  /**
   * Test hook: how many live listeners an event has (deterministic
   * emit-timing gate for the fixture-snapshot rows).
   * @param {string} event
   * @returns {number}
   */
  function listenerCount(event) {
    return listeners.get(event)?.size ?? 0;
  }
  /** @type {any} */ (window).__e2eListenerCount = listenerCount;
})();
