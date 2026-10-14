// src/lib/transport.ts — one surface, two backends. Play UI imports ONLY this.
import type { Intent, PatchEvent, PresenceEvent } from "$lib/ipc";

export interface Transport {
  readonly mode: "ipc" | "ws";
  connect(): Promise<void>;
  send(intent: Intent): Promise<void>;
  onPatch(cb: (p: PatchEvent) => void): () => void;
  onPresence(cb: (p: PresenceEvent) => void): () => void;
  onError(cb: (msg: string) => void): () => void;
  close(): Promise<void>;
}

/** Desktop host/peer path: IPC + listen("session:patch" / "session:presence"). */
export function ipcTransport(): Transport {
  let patchCallbacks: ((p: PatchEvent) => void)[] = [];
  let presenceCallbacks: ((p: PresenceEvent) => void)[] = [];
  let errorCallbacks: ((msg: string) => void)[] = [];
  let unlistenPatch: (() => void) | null = null;
  let unlistenPresence: (() => void) | null = null;

  return {
    mode: "ipc",
    async connect() {
      const { listen } = await import("@tauri-apps/api/event");
      unlistenPatch = await listen<PatchEvent>("session:patch", (event) => {
        patchCallbacks.forEach((cb) => cb(event.payload));
      });
      unlistenPresence = await listen<PresenceEvent>("session:presence", (event) => {
        presenceCallbacks.forEach((cb) => cb(event.payload));
      });
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
    async close() {
      unlistenPatch?.();
      unlistenPresence?.();
      unlistenPatch = null;
      unlistenPresence = null;
    },
  };
}

/** Mobile thin client: WebSocket to host lan_url, frames per §4.3. */
export function wsTransport(lanUrl: string, name: string): Transport {
  let ws: WebSocket | null = null;
  let patchCallbacks: ((p: PatchEvent) => void)[] = [];
  let presenceCallbacks: ((p: PresenceEvent) => void)[] = [];
  let errorCallbacks: ((msg: string) => void)[] = [];
  let connectResolve: (() => void) | null = null;
  let connectReject: ((e: Error) => void) | null = null;
  let connected = false;

  const url = new URL(lanUrl);
  url.pathname = `/ws${url.pathname}`;

  return {
    mode: "ws",
    connect() {
      return new Promise<void>((resolve, reject) => {
        connectResolve = resolve;
        connectReject = reject;
        ws = new WebSocket(url.toString());
        ws.onopen = () => {
          ws!.send(JSON.stringify({ type: "hello", session_id: url.pathname.slice(4), name }));
        };
        ws.onmessage = (event) => {
          try {
            const frame = JSON.parse(event.data);
            switch (frame.type) {
              case "welcome":
                connected = true;
                connectResolve?.();
                connectResolve = null;
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
          connectReject?.(new Error("WebSocket connection failed"));
        };
        ws.onclose = () => {
          connected = false;
          connectReject?.(new Error("WebSocket closed"));
        };
      });
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
    async close() {
      ws?.close();
      ws = null;
      connected = false;
    },
  };
}

/** Test double for vitest; records intents, feeds patches from a queue. */
export function mockTransport(): Transport & { sent: Intent[]; emit(p: PatchEvent): void; emitPresence(p: PresenceEvent): void; emitError(msg: string): void } {
  const sent: Intent[] = [];
  let patchCallbacks: ((p: PatchEvent) => void)[] = [];
  let presenceCallbacks: ((p: PresenceEvent) => void)[] = [];
  let errorCallbacks: ((msg: string) => void)[] = [];

  return {
    mode: "ipc",
    sent,
    async connect() {},
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
    emit(p: PatchEvent) {
      patchCallbacks.forEach((cb) => cb(p));
    },
    emitPresence(p: PresenceEvent) {
      presenceCallbacks.forEach((cb) => cb(p));
    },
    emitError(msg: string) {
      errorCallbacks.forEach((cb) => cb(msg));
    },
    async close() {},
  };
}

/** Backend selection: Tauri IPC if available, else WS from `?host=` query param. */
export function createTransport(name: string): Transport {
  // @ts-expect-error Tauri injects this global in desktop builds
  if (typeof window !== "undefined" && window.__TAURI_INTERNALS__) {
    return ipcTransport();
  }
  const params = new URLSearchParams(window.location.search);
  const host = params.get("host");
  if (host) {
    return wsTransport(host, name);
  }
  throw new Error("No transport available: not in Tauri and no ?host= query param");
}