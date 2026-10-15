// Mock for "@tauri-apps/api/event" — bundled ONLY in `harness` mode via
// vite.config.js resolve.alias (PLAN §3.1, §3.2).
//
// A per-event listener registry. `listen()` registers a handler and returns
// an unlisten function, matching the Tauri API surface used by src/:
//   - src/lib/llm.svelte.ts: listen("llm:token", …)
//   - src/lib/transport.ts:  (await import("@tauri-apps/api/event")).listen(
//       "session:patch" | "session:presence", …)
// `window.__pf2eHarness.emit()` (installed by tauri-core.ts) dispatches
// synchronously into this registry.

export type UnlistenFn = () => void;

/** Mirrors the fields of Tauri's Event<T> that src/ touches. */
export interface HarnessEvent<T> {
  event: string;
  id: number;
  payload: T;
}

type Handler = (event: HarnessEvent<never>) => void;

const registry = new Map<string, Map<number, Handler>>();
let nextListenerId = 1;

/** App-side emit() calls are recorded here; nothing in src/ uses emit today. */
const emitted: Array<{ event: string; payload: unknown }> = [];

export function listenerCount(event: string): number {
  return registry.get(event)?.size ?? 0;
}

export function dispatch(event: string, payload: unknown): void {
  const handlers = registry.get(event);
  if (!handlers) return;
  // Snapshot so an unlisten() inside a handler doesn't mutate mid-dispatch.
  for (const [id, handler] of [...handlers]) {
    handler({ event, id, payload } as HarnessEvent<never>);
  }
}

export async function listen<T>(
  event: string,
  handler: (event: HarnessEvent<T>) => void,
): Promise<UnlistenFn> {
  let handlers = registry.get(event);
  if (!handlers) {
    handlers = new Map();
    registry.set(event, handlers);
  }
  const id = nextListenerId++;
  handlers.set(id, handler as Handler);
  return () => {
    handlers.delete(id);
  };
}

/** Tauri app→backend emit: records and no-ops (PLAN §3.2 point 4). */
export async function emit(event: string, payload?: unknown): Promise<void> {
  emitted.push({ event, payload });
}
