// Playwright-side driver for the live LAN lane (PLAN §3.4): spawns
// src-tauri/examples/e2e_host.rs — the real Rust `lan` WebSocket server over
// tokio-tungstenite — and speaks its NDJSON stdio protocol:
//
//   stdout  {"type":"ready","port":N,"session_id":"lan-test",…}  (first line)
//   stdin   {"intent": <Intent>}
//   stdout  {"type":"patch","event":{…}} × the applied broadcast-derived events
//   stdout  {"type":"applied","ok":true|false[,"error":…]}
//
// All waits are bounded; effects are awaited on the ack line, never slept.
// The lane is gated on PF2E_E2E_LAN=1 (pnpm e2e:live), mirroring the
// PF2E_NET_TESTS pattern in src-tauri/tests/phase10_p2p.rs.
import type { Intent } from "$lib/ipc";
import { fixtureSessionPin } from "../fixtures/session";

export const LIVE_LANE_ENV = "PF2E_E2E_LAN";

/** True when the loopback live lane is opted in. */
export function liveLaneEnabled(): boolean {
  // @ts-expect-error — Node global; @types/node is intentionally absent
  // (tsconfig compilerOptions.types:[] — see PLAN §9 12.0 note).
  return process.env[LIVE_LANE_ENV] === "1";
}

/** Structural subset of node:child_process's ChildProcess this driver uses. */
interface ChildProc {
  stdin: { write(chunk: string): unknown; end(): void };
  stdout: { on(event: "data", cb: (chunk: { toString(): string }) => void): unknown };
  stderr: { on(event: "data", cb: (chunk: { toString(): string }) => void): unknown };
  on(event: "exit", cb: (code: number | null) => void): unknown;
  kill(signal?: string): unknown;
}

interface ReadyLine {
  type: "ready";
  port: number;
  session_id: string;
  name: string;
  pin: string;
}

/** One patch line from the example's stdout — only the fields the specs
 *  read are typed; the payload shapes are owned by src-tauri (and mirrored
 *  in $lib/ipc's PatchEvent). */
export interface LivePatchEvent {
  kind: string;
  session_id: string;
  entry?: { kind: string; text: string; roll: unknown };
  slug?: string;
}

export interface LiveHost {
  port: number;
  sessionId: string;
  sessionName: string;
  pin: string;
  /** Relative /table/play URL for a LAN player against this host. */
  playerUrl(name: string, pin?: string): string;
  /** Apply one GM intent over stdin NDJSON; resolves with the patch events the
   *  apply produced (rejects with the authority error message on failure).
   *  Bounded wait; serialize calls — the protocol is one ack per intent. */
  gmIntent(intent: Intent): Promise<LivePatchEvent[]>;
  /** Close stdin (the example exits on EOF, freeing the port); bounded. */
  close(): Promise<void>;
}

type JsonLine = Record<string, unknown>;

/**
 * Spawn the example host and await its ready line. The first spawn may
 * compile the example via cargo — the ready wait is generous on purpose
 * (bounded at 3 min); subsequent spawns are instant.
 */
export async function spawnLiveHost(opts?: { pin?: string }): Promise<LiveHost> {
  if (!liveLaneEnabled()) {
    throw new Error(`liveHost: ${LIVE_LANE_ENV}=1 is required (run pnpm e2e:live)`);
  }
  const pin = opts?.pin ?? fixtureSessionPin;
  // src-tauri/ from tests/e2e/server/ — URL math keeps node:url/node:path
  // out of the type-checked surface (unix dev shells per PLAN §7).
  const srcTauri = new URL("../../../src-tauri/", import.meta.url).pathname;
  // Post-merge the shim (e2e/node-shims.d.ts) types node:child_process;
  // ChildProc narrows its surface to what this driver uses.
  const cp = await import("node:child_process");
  const spawn = cp.spawn as (
    cmd: string,
    args: string[],
    opts: { cwd: string; stdio: ["pipe", "pipe", "pipe"] },
  ) => ChildProc;
  const proc = spawn(
    "cargo",
    ["run", "--quiet", "--example", "e2e_host", "--", "--pin", pin],
    { cwd: srcTauri, stdio: ["pipe", "pipe", "pipe"] },
  );

  // --- NDJSON line pump -------------------------------------------------
  let buffer = "";
  let exited: number | null = null;
  let stderrTail = "";
  const pending = new Map<
    (line: JsonLine) => boolean,
    { reject: (e: Error) => void; timer: ReturnType<typeof setTimeout> }
  >();

  const failPending = (why: string) => {
    for (const [, p] of pending) {
      clearTimeout(p.timer);
      p.reject(new Error(`liveHost: ${why}\nstderr tail:\n${stderrTail}`));
    }
    pending.clear();
  };

  proc.stderr.on("data", (c) => {
    stderrTail = (stderrTail + c.toString()).slice(-4096);
  });
  proc.on("exit", (code) => {
    exited = code;
    failPending(`e2e_host exited (${String(code)})`);
  });
  proc.stdout.on("data", (chunk) => {
    buffer += chunk.toString();
    let idx = buffer.indexOf("\n");
    while (idx >= 0) {
      const line = buffer.slice(0, idx).trim();
      buffer = buffer.slice(idx + 1);
      idx = buffer.indexOf("\n");
      if (!line) continue;
      let parsed: JsonLine;
      try {
        parsed = JSON.parse(line) as JsonLine;
      } catch {
        continue; // not one of ours; ignore
      }
      for (const [matcher] of [...pending]) {
        if (matcher(parsed)) pending.delete(matcher);
      }
    }
  });

  function waitFor<T>(
    match: (line: JsonLine) => T | null,
    ms: number,
    what: string,
  ): Promise<T> {
    if (exited !== null) {
      return Promise.reject(
        new Error(`liveHost: e2e_host already exited; cannot await ${what}`),
      );
    }
    return new Promise<T>((resolve, reject) => {
      const timer = setTimeout(() => {
        pending.delete(matcher);
        reject(
          new Error(
            `liveHost: timed out after ${ms}ms awaiting ${what}\nstderr tail:\n${stderrTail}`,
          ),
        );
      }, ms);
      const matcher = (line: JsonLine): boolean => {
        const v = match(line);
        if (v === null) return false;
        clearTimeout(timer);
        resolve(v);
        return true;
      };
      pending.set(matcher, { reject, timer });
    });
  }

  const ready = await waitFor<ReadyLine>(
    (line) =>
      line.type === "ready" && typeof line.port === "number"
        ? (line as unknown as ReadyLine)
        : null,
    180_000,
    "the e2e_host ready line (first spawn compiles the example via cargo)",
  );

  async function gmIntent(intent: Intent): Promise<LivePatchEvent[]> {
    const patches: LivePatchEvent[] = [];
    const ack = waitFor<{ ok: boolean; error?: string }>(
      (line) => {
        if (line.type === "patch") {
          patches.push(line.event as LivePatchEvent);
          return null;
        }
        if (line.type === "applied") {
          return {
            ok: line.ok === true,
            error: typeof line.error === "string" ? line.error : undefined,
          };
        }
        return null;
      },
      15_000,
      `"applied" ack for intent ${intent.kind}`,
    );
    proc.stdin.write(JSON.stringify({ intent }) + "\n");
    const result = await ack;
    if (!result.ok) {
      throw new Error(`liveHost: authority rejected the intent: ${result.error}`);
    }
    return patches;
  }

  function playerUrl(name: string, pinOverride?: string): string {
    const hostUrl = `ws://127.0.0.1:${ready.port}/${ready.session_id}`;
    const p = pinOverride ?? ready.pin;
    return (
      `/table/play?name=${encodeURIComponent(name)}` +
      `&host=${encodeURIComponent(hostUrl)}` +
      `&pin=${encodeURIComponent(p)}`
    );
  }

  async function close(): Promise<void> {
    if (exited !== null) return;
    // Stdin EOF is the example's orderly shutdown: its intent loop breaks,
    // main returns, the listener drops, the port is freed. SIGKILL is only
    // the bounded fallback.
    const done = new Promise<void>((resolve) => {
      const timer = setTimeout(() => {
        proc.kill("SIGKILL");
        resolve();
      }, 5_000);
      proc.on("exit", () => {
        clearTimeout(timer);
        resolve();
      });
    });
    proc.stdin.end();
    await done;
  }

  return {
    port: ready.port,
    sessionId: ready.session_id,
    sessionName: ready.name,
    pin: ready.pin,
    playerUrl,
    gmIntent,
    close,
  };
}
