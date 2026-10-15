import { defineConfig, devices } from "@playwright/test";

// Phase 12 E2E harness — deterministic Chromium lane (docs/phase12/PLAN.md).
// The webServer builds the app with the Tauri API aliased to in-repo mocks
// (--mode harness) and serves it via `vite preview` on loopback.
export default defineConfig({
  testDir: "tests/e2e",
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: [["list"], ["html", { outputFolder: "playwright-report", open: "never" }]],
  use: {
    baseURL: "http://127.0.0.1:4173",
    trace: "retain-on-failure",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: "pnpm build:harness && pnpm preview --port 4173 --strictPort",
    url: "http://127.0.0.1:4173",
    reuseExistingServer: !process.env.CI,
    timeout: 180_000,
  },
});
