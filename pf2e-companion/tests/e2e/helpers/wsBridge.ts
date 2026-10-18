// Deterministic WS-lane mock (F-R7; docs/plans/PLAN-F-R7-peer-outbound.md).
//
// The existing lanes sit at two ends: the gameflow lane plays the whole
// authority through the mocked IPC event emitters, and the live lane talks to
// the real Rust bridge. This helper fills the middle: a `tauri:false` page
// runs its REAL wsTransport against a MockWebSocket installed by init script
// (the harness's existing approach — tauri-core.ts installs __TAURI_INTERNALS__
// the same way). The page's frames are recorded verbatim; the test plays the
// host by `deliver`ing HostFrame-shaped JSON, so the page's parse/reducer path
// is exercised exactly as on the wire.
//
// Timing discipline mirrors the IPC lane: the mock opens asynchronously (a
// macrotask, like a real socket), tests await the recorded `hello` before
// delivering `welcome`/`snapshot`, and DOM assertions use bounded polls.
import { expect, type Page } from "@playwright/test";
import type { Intent } from "$lib/ipc";

/** One recorded client→host frame, parsed. */
export type WsFrame = Record<string, unknown> & { type?: string };

/**
 * Replace `window.WebSocket` with a recording mock and install the
 * `window.__pf2eWs` test surface BEFORE the page's scripts run. Pair with
 * `installHarness(page, { tauri: false })` so createTransport picks
 * wsTransport.
 */
export async function installWsMock(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const sent: string[] = [];
    let current: {
      url: string;
      readyState: number;
      onmessage: ((ev: { data: string }) => void) | null;
      onclose: (() => void) | null;
    } | null = null;

    class MockWebSocket {
      static readonly CONNECTING = 0;
      static readonly OPEN = 1;
      static readonly CLOSING = 2;
      static readonly CLOSED = 3;

      readonly url: string;
      readyState = MockWebSocket.CONNECTING;
      onopen: (() => void) | null = null;
      onmessage: ((ev: { data: string }) => void) | null = null;
      onerror: ((ev: unknown) => void) | null = null;
      onclose: (() => void) | null = null;

      constructor(url: string) {
        this.url = url;
        current = this;
        // Real sockets never open synchronously — the transport attaches
        // its handlers right after construction, then expects onopen on a
        // later task.
        setTimeout(() => {
          if (this.readyState === MockWebSocket.CONNECTING) {
            this.readyState = MockWebSocket.OPEN;
            this.onopen?.();
          }
        }, 0);
      }

      send(data: string | ArrayBufferLike | Blob | ArrayBufferView): void {
        if (this.readyState !== MockWebSocket.OPEN) return;
        sent.push(String(data));
      }

      close(): void {
        this.readyState = MockWebSocket.CLOSED;
        this.onclose?.();
      }
    }

    window.__pf2eWs = {
      sent,
      deliver(frame: unknown): void {
        current?.onmessage?.({ data: JSON.stringify(frame) });
      },
      lastUrl(): string | null {
        return current?.url ?? null;
      },
    };
    // @ts-expect-error — deliberate full replacement of the global for the
    // deterministic WS lane; the mock implements the members wsTransport reads.
    window.WebSocket = MockWebSocket;
  });
}

/** All frames the page has sent so far, parsed in send order. */
export async function wsSentFrames(page: Page): Promise<WsFrame[]> {
  const raw = await page.evaluate(() => {
    if (!window.__pf2eWs) throw new Error("installWsMock was not called");
    return window.__pf2eWs.sent;
  });
  return raw.map((s) => JSON.parse(s) as WsFrame);
}

/**
 * Bounded poll for a sent frame matching `pred` (the §3.2 readiness-poll
 * style — wsTransport sends from event handlers, so a one-shot read after a
 * click can race the mock's task hop). Resolves with the matched frame.
 */
export async function awaitWsFrame(
  page: Page,
  pred: (frame: WsFrame) => boolean,
  what: string,
): Promise<WsFrame> {
  let found: WsFrame | undefined;
  await expect
    .poll(
      async () => {
        found = (await wsSentFrames(page)).find(pred);
        return found !== undefined;
      },
      { message: `expected the page to send a ws frame: ${what}` },
    )
    .toBe(true);
  return found as WsFrame;
}

/**
 * Assert the page sent ≥1 `{"type":"intent","intent":{…}}` frame whose
 * intent has `kind` and matches `matcher` (objectContaining) — the
 * ws-transport mirror of helpers/harness.ts `expectIntents`. The matcher is
 * part of the bounded poll: a second call for the same kind must wait for
 * ITS frame, not re-match the first one.
 */
export async function expectWsIntent(
  page: Page,
  kind: Intent["kind"],
  matcher: Record<string, unknown> = {},
): Promise<void> {
  const entries = Object.entries(matcher);
  await awaitWsFrame(
    page,
    (f) =>
      f.type === "intent" &&
      typeof f.intent === "object" &&
      f.intent !== null &&
      (f.intent as Record<string, unknown>).kind === kind &&
      entries.every(
        ([k, v]) =>
          JSON.stringify((f.intent as Record<string, unknown>)[k]) ===
          JSON.stringify(v),
      ),
    `an intent frame of kind "${kind}" matching ${JSON.stringify(matcher)}`,
  );
}

/** Deliver one host→client frame through the page's wsTransport. */
export async function deliverWsFrame(
  page: Page,
  frame: WsFrame,
): Promise<void> {
  await page.evaluate((f) => {
    if (!window.__pf2eWs) throw new Error("installWsMock was not called");
    window.__pf2eWs.deliver(f);
  }, frame);
}
