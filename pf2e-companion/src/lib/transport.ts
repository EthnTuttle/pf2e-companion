// src/lib/transport.ts — one surface, two backends. Play UI imports ONLY this.
import type { Intent, PatchEvent, PresenceEvent } from "$lib/ipc";

export type ConnectionState =
  | "offline"
  | "connecting"
  | "connected"
  | "reconnecting"
  | "failed";

export interface Transport {
  readonly mode: "ipc" | "ws";
  connect(): Promise<void>;
  send(intent: Intent): Promise<void>;
  onPatch(cb: (p: PatchEvent) => void): () => void;
  onPresence(cb: (p: PresenceEvent) => void): () => void;
  onError(cb: (msg: string) => void): () => void;
  onStateChange(cb: (state: ConnectionState, attempt: number) => void): () => void;
  retryConnection(): Promise<void>;
  close(): Promise<void>;
}

/** Desktop host/peer path: IPC + listen("session:patch" / "session:presence"). */
export function ipcTransport(): Transport {
  let patchCallbacks: ((p: PatchEvent) => void)[] = [];
  let presenceCallbacks: ((p: PresenceEvent) => void)[] = [];
  let errorCallbacks: ((msg: string) => void)[] = [];
  let stateCallbacks: ((state: ConnectionState, attempt: number) => void)[] = [];
  let unlistenPatch: (() => void) | null = null;
  let unlistenPresence: (() => void) | null = null;

  const emitState = (state: ConnectionState, attempt = 0) => {
    stateCallbacks.forEach((cb) => cb(state, attempt));
  };

  return {
    mode: "ipc",
    async connect() {
      emitState("connecting");
      const { listen } = await import("@tauri-apps/api/event");
      unlistenPatch = await listen<PatchEvent>("session:patch", (event) => {
        patchCallbacks.forEach((cb) => cb(event.payload));
      });
      unlistenPresence = await listen<PresenceEvent>("session:presence", (event) => {
        presenceCallbacks.forEach((cb) => cb(event.payload));
      });
      emitState("connected");
    },
    async send(intent: Intent) {
      const { sessionIntent } = await import("$lib/ipc");
      await sessionIntent(intent);
    },
    onPatch(cb) {
      patchCallbacks.push(cb);
      return () => {
        patchCallbacks = patchCallbacks.filter((c) => c !== cb);
      };
    },
    onPresence(cb) {
      presenceCallbacks.push(cb);
      return () => {
        presenceCallbacks = presenceCallbacks.filter((c) => c !== cb);
      };
    },
    onError(cb) {
      errorCallbacks.push(cb);
      return () => {
        errorCallbacks = errorCallbacks.filter((c) => c !== cb);
      };
    },
    onStateChange(cb) {
      stateCallbacks.push(cb);
      return () => {
        stateCallbacks = stateCallbacks.filter((c) => c !== cb);
      };
    },
    async retryConnection() {
      // IPC transport doesn't need reconnection
    },
    async close() {
      unlistenPatch?.();
      unlistenPresence?.();
      unlistenPatch = null;
      unlistenPresence = null;
      emitState("offline");
    },
  };
}

/** Mobile thin client: WebSocket to host lan_url, frames per §4.3. */
export function wsTransport(lanUrl: string, name: string, pin?: string): Transport {
  let ws: WebSocket | null = null;
  let patchCallbacks: ((p: PatchEvent) => void)[] = [];
  let presenceCallbacks: ((p: PresenceEvent) => void)[] = [];
  let errorCallbacks: ((msg: string) => void)[] = [];
  let stateCallbacks: ((state: ConnectionState, attempt: number) => void)[] = [];
  let connectResolve: (() => void) | null = null;
  let connectReject: ((e: Error) => void) | null = null;
  let connected = false;
  let manualClose = false;
  let reconnectAttempt = 0;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  const maxReconnectAttempts = 3;
  const backoffMs = [500, 1000, 2000];

  const url = new URL(lanUrl);
  url.pathname = `/ws${url.pathname}`;

  const emitState = (state: ConnectionState, attempt = 0) => {
    stateCallbacks.forEach((cb) => cb(state, attempt));
  };

  const clearReconnectTimer = () => {
    if (reconnectTimer !== null) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
  };

  const doConnect = (): Promise<void> => {
    return new Promise<void>((resolve, reject) => {
      connectResolve = resolve;
      connectReject = reject;
      manualClose = false;

      ws = new WebSocket(url.toString());
      ws.onopen = () => {
        const hello: Record<string, unknown> = {
          type: "hello",
          session_id: url.pathname.slice(4),
          name,
        };
        if (pin) {
          hello.pin = pin;
        }
        ws!.send(JSON.stringify(hello));
      };
      ws.onmessage = (event) => {
        try {
          const frame = JSON.parse(event.data);
          switch (frame.type) {
            case "welcome":
              connected = true;
              reconnectAttempt = 0;
              connectResolve?.();
              connectResolve = null;
              emitState("connected");
              break;
            case "patch":
              patchCallbacks.forEach((cb) => cb(frame));
              break;
            case "presence":
              presenceCallbacks.forEach((cb) => cb(frame));
              break;
            case "error":
              if (!connected) {
                connectReject?.(new Error(frame.message));
              } else {
                errorCallbacks.forEach((cb) => cb(frame.message));
              }
              break;
          }
        } catch {
          // ignore malformed frames
        }
      };
      ws.onerror = (e) => {
        if (!connected) {
          connectReject?.(new Error("WebSocket connection failed"));
        }
      };
      ws.onclose = () => {
        connected = false;
        if (!manualClose) {
          handleUnexpectedClose();
        } else {
          connectReject?.(new Error("WebSocket closed"));
          emitState("offline");
        }
      };
    });
  };

  const handleUnexpectedClose = () => {
    if (reconnectAttempt >= maxReconnectAttempts) {
      emitState("failed", reconnectAttempt);
      errorCallbacks.forEach((cb) => cb("Connection lost after 3 attempts"));
      return;
    }

    emitState("reconnecting", reconnectAttempt + 1);

    const delay = backoffMs[reconnectAttempt] ?? 2000;
    reconnectAttempt++;
    reconnectTimer = setTimeout(() => {
      doConnect().catch(() => {
        // Error handled in onclose/onerror
      });
    }, delay);
  };

  return {
    mode: "ws",
    connect() {
      return doConnect();
    },
    async send(intent: Intent) {
      if (!ws || ws.readyState !== WebSocket.OPEN) {
        throw new Error("WebSocket not connected");
      }
      ws.send(JSON.stringify({ type: "intent", intent }));
    },
    onPatch(cb) {
      patchCallbacks.push(cb);
      return () => {
        patchCallbacks = patchCallbacks.filter((c) => c !== cb);
      };
    },
    onPresence(cb) {
      presenceCallbacks.push(cb);
      return () => {
        presenceCallbacks = presenceCallbacks.filter((c) => c !== cb);
      };
    },
    onError(cb) {
      errorCallbacks.push(cb);
      return () => {
        errorCallbacks = errorCallbacks.filter((c) => c !== cb);
      };
    },
    onStateChange(cb) {
      stateCallbacks.push(cb);
      return () => {
        stateCallbacks = stateCallbacks.filter((c) => c !== cb);
      };
    },
    async retryConnection() {
      clearReconnectTimer();
      reconnectAttempt = 0;
      manualClose = false;
      emitState("connecting");
      await doConnect();
    },
    async close() {
      manualClose = true;
      clearReconnectTimer();
      ws?.close();
      ws = null;
      connected = false;
      emitState("offline");
    },
  };
}

/** Test double for vitest; records intents, feeds patches from a queue. */
export function mockTransport(): Transport & {
  sent: Intent[];
  emit(p: PatchEvent): void;
  emitPresence(p: PresenceEvent): void;
  emitError(msg: string): void;
} {
  const sent: Intent[] = [];
  let patchCallbacks: ((p: PatchEvent) => void)[] = [];
  let presenceCallbacks: ((p: PresenceEvent) => void)[] = [];
  let errorCallbacks: ((msg: string) => void)[] = [];
  let stateCallbacks: ((state: ConnectionState, attempt: number) => void)[] = [];

  const emitState = (state: ConnectionState, attempt = 0) => {
    stateCallbacks.forEach((cb) => cb(state, attempt));
  };

  return {
    mode: "ipc",
    sent,
    async connect() {
      emitState("connected");
    },
    async send(intent: Intent) {
      sent.push(intent);
    },
    onPatch(cb) {
      patchCallbacks.push(cb);
      return () => {
        patchCallbacks = patchCallbacks.filter((c) => c !== cb);
      };
    },
    onPresence(cb) {
      presenceCallbacks.push(cb);
      return () => {
        presenceCallbacks = presenceCallbacks.filter((c) => c !== cb);
      };
    },
    onError(cb) {
      errorCallbacks.push(cb);
      return () => {
        errorCallbacks = errorCallbacks.filter((c) => c !== cb);
      };
    },
    onStateChange(cb) {
      stateCallbacks.push(cb);
      return () => {
        stateCallbacks = stateCallbacks.filter((c) => c !== cb);
      };
    },
    async retryConnection() {
      emitState("connected");
    },
    emit(p: PatchEvent) {
      patchCallbacks.forEach((cb) => cb(p));
    },
    emitPresence(p: PresenceEvent) {
      presenceCallbacks.forEach((cb) => cb(p));
    },
    emitError(msg: string) {
      errorCallbacks.forEach((cb) => cb(msg));
    },
    async close() {
      emitState("offline");
    },
  };
}

/** Backend selection: an explicit `?host=` LAN target wins in EVERY shell —
 *  the native webview can open ws:// directly (CSP null, cleartext allowed
 *  on Android), so a LAN joiner is a thin WS client even inside the app
 *  (F-R1). Only with no host param does Tauri IPC take over; a bare browser
 *  with neither fails loudly. */
export function createTransport(name: string): Transport {
  if (typeof window !== "undefined") {
    const params = new URLSearchParams(window.location.search);
    const host = params.get("host");
    const pin = params.get("pin");
    if (host) {
      return wsTransport(host, name, pin ?? undefined);
    }
    // @ts-expect-error Tauri injects this global in desktop builds
    if (window.__TAURI_INTERNALS__) {
      return ipcTransport();
    }
  }
  throw new Error("No transport available: not in Tauri and no ?host= query param");
}