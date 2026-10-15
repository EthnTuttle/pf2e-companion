import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
// e2e/node-shims.d.ts declares node:url post-merge; process stays shimmed-out
// (declared locally in e2e/shots.ts), so its guard below remains.
import { fileURLToPath } from "node:url";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async ({ mode }) => ({
  plugins: [sveltekit()],

  // Phase 12 E2E harness (docs/phase12/PLAN.md §3.1): in `--mode harness`
  // only, the app's two — and only two — Tauri API touchpoints are aliased
  // to in-repo mocks wired to window.__PF2E_HARNESS_CONFIG__/.__pf2eHarness.
  // Production builds (dev/build/tauri) never pass this mode.
  resolve:
    mode === "harness"
      ? {
          alias: {
            "@tauri-apps/api/core": fileURLToPath(
              new URL("./tests/e2e/harness/tauri-core.ts", import.meta.url),
            ),
            "@tauri-apps/api/event": fileURLToPath(
              new URL("./tests/e2e/harness/tauri-event.ts", import.meta.url),
            ),
          },
        }
      : undefined,

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },

  // Vitest config (`pnpm test`).
  // Pure-TS unit tests live alongside source as `*.test.ts`.
  // Skip Svelte-component tests for now — they need jsdom + Testing Library
  // and the citation parser doesn't need a DOM.
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
    globals: false,
  },
}));
