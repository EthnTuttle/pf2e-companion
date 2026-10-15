// Mock for "@tauri-apps/api/core" — bundled ONLY in `harness` mode via
// vite.config.js resolve.alias (PLAN §3.1, §3.2). Side-effect module: at
// import time it installs window.__pf2eHarness and (unless the config says
// otherwise, for the WS/live lanes) the __TAURI_INTERNALS__ flag that makes
// transport.ts:createTransport() pick ipcTransport().
//
// Fixture *data* arrives via page.addInitScript (JSON-serializable only);
// fixture *behavior* — this dispatcher — is bundled code. addInitScript runs
// before page module scripts, so __PF2E_HARNESS_CONFIG__ exists before any
// route's onMount can invoke.
//
// Stub semantics (PLAN §3.2 point 3):
//   { ok: value }   → resolve with value
//   { error: msg }  → reject with new Error(msg)
//   { queue: [...] } → shift once per call; empty queue is a hard error
//   missing key     → reject with `harness: no stub for "<cmd>"` (fail fast:
//                     an unstubbed command is a fixture bug, not an error path
//                     to screenshot as evidence)

import { dispatch, listenerCount } from "./tauri-event";

if (window.__PF2E_HARNESS_CONFIG__?.tauri !== false) {
  // Local cast on purpose: __TAURI_INTERNALS__ stays off the global.d.ts
  // Window interface because transport.ts / transport.test.ts carry
  // ts-expect-error directives that need the property untyped (see the
  // NOTE in global.d.ts). Runtime behavior per PLAN §3.2 point 1.
  (window as { __TAURI_INTERNALS__?: Record<string, never> })
    .__TAURI_INTERNALS__ ??= {};
}

window.__pf2eHarness = {
  invocations: [],
  listenerCount,
  emit(event: string, payload: unknown): void {
    dispatch(event, payload);
  },
  reset(): void {
    window.__pf2eHarness.invocations.length = 0;
  },
};

function resolveStub(cmd: string, stub: Stub): unknown {
  if ("ok" in stub) return stub.ok;
  if ("error" in stub) throw new Error(stub.error);
  const next = stub.queue.shift();
  if (next === undefined) {
    throw new Error(`harness: stub queue for "${cmd}" is empty`);
  }
  return resolveStub(cmd, next);
}

export async function invoke<T>(cmd: string, args?: unknown): Promise<T> {
  window.__pf2eHarness.invocations.push({ cmd, args });
  const stub = window.__PF2E_HARNESS_CONFIG__?.responses[cmd];
  if (stub === undefined) {
    throw new Error(`harness: no stub for "${cmd}"`);
  }
  return resolveStub(cmd, stub) as T;
}
