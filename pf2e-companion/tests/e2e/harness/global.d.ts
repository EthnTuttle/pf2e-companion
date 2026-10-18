// Wire protocol shared between the in-page mock modules (harness/tauri-*.ts)
// and the Playwright-side helpers (helpers/harness.ts). PLAN §3.2, §3.3.
//
// This file is ambient (no imports/exports) so both sides — the app bundle
// built in `harness` mode and the Playwright runner — see the same types
// without a cross-boundary import.

/** One stubbed IPC response: fixed payload, thrown error, or a shift-once queue. */
type Stub = { ok: unknown } | { error: string } | { queue: Stub[] };

/** One recorded mocked-IPC call (append-only, read via helpers' invocations()). */
interface InvocationRecord {
  cmd: string;
  args: unknown;
}

interface Window {
  /** Seeded by page.addInitScript before any page module runs. */
  __PF2E_HARNESS_CONFIG__?: {
    responses: Record<string, Stub>;
    /**
     * `false` in the WS/live lanes (PLAN §3.4 S26, L1–L4): the Tauri flag is
     * left unset so createTransport() picks wsTransport instead of ipcTransport.
     */
    tauri?: boolean;
  };
  __pf2eHarness: {
    /** Append-only, newest last. Cleared by reset(). */
    invocations: InvocationRecord[];
    /** Readiness probe: how many listeners are registered for `event`. */
    listenerCount(event: string): number;
    /** Synchronously dispatch `payload` to all listeners of `event`. */
    emit(event: string, payload: unknown): void;
    /** Clear the invocation record (between phases). */
    reset(): void;
  };
  /**
   * The deterministic WS-lane mock (F-R7; tests/e2e/helpers/wsBridge.ts).
   * Present only after `installWsMock(page)` — it replaces window.WebSocket
   * with a recording mock so a `tauri:false` page's REAL wsTransport can
   * complete its handshake against the test-as-authority.
   */
  __pf2eWs?: {
    /** Every frame the page's wsTransport sent, raw JSON, oldest first. */
    sent: string[];
    /**
     * Push one host→client frame through the page's wsTransport (the mock's
     * `onmessage`). Mirrors `HostFrame`'s serde: patch/presence frames carry
     * the event fields flattened beside `type` (src-tauri/src/lan.rs).
     */
    deliver(frame: unknown): void;
    /** URL the page dialed (the `/ws/`-prefixed wire form), or null pre-connect. */
    lastUrl(): string | null;
  };
  /**
   * NOTE: `__TAURI_INTERNALS__` (PLAN §3.2 point 1) is deliberately NOT
   * declared here. src/lib/transport.ts and src/lib/transport.test.ts carry
   * `@ts-expect-error` directives that require the property to stay untyped;
   * declaring it would orphan those directives (error: unused directive).
   * tauri-core.ts sets the flag through a local cast instead.
   */
}
