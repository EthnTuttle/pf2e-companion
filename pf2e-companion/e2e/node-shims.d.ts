// e2e/node-shims.d.ts — minimal ambient types for the node builtins the
// screenshot runner touches. The project deliberately ships without
// @types/node (the standing `types: ["node"]` svelte-check warning), so the
// harness declares just what it uses. Runtime is real Node 24. (`process`
// is declared locally in shots.ts to avoid changing vite.config.js typing.)
declare module "node:child_process" {
  export interface ChildProcess {
    stdout: { on(event: string, cb: (data: unknown) => void): void } | null;
    stderr: { on(event: string, cb: (data: unknown) => void): void } | null;
    // liveHost (tests/e2e/server) drives stdin/on-exit; widened post-merge so
    // its structural ChildProc narrowing cast stays legal.
    stdin: { write(chunk: string): unknown; end(): void } | null;
    readonly exitCode: number | null;
    on(event: string, cb: (code: number | null) => void): void;
    kill(signal?: string): void;
  }
  export function spawn(
    command: string,
    args: string[],
    options?: { cwd?: string; stdio?: unknown },
  ): ChildProcess;
  export function spawnSync(
    command: string,
    args: string[],
    options?: { cwd?: string; stdio?: unknown },
  ): { status: number | null };
}

declare module "node:fs" {
  export function mkdirSync(path: string, options: { recursive: boolean }): void;
  export function rmSync(path: string, options: { recursive: boolean; force: boolean }): void;
  export function writeFileSync(path: string, data: Uint8Array | string): void;
}

declare module "node:module" {
  export function createRequire(filename: string): (id: string) => unknown;
}

declare module "node:path" {
  export function dirname(p: string): string;
  export function join(...parts: string[]): string;
}

declare module "node:url" {
  // Real node accepts string|URL; tests/e2e + vite.config.js pass URL objects.
  export function fileURLToPath(url: string | URL): string;
}
