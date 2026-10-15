// e2e/shots.ts — the screenshot runner (node e2e/shots.ts via pnpm e2e:shots).
//
// Builds the static bundle, serves it with `vite preview` (prod CSS, no HMR
// websocket), boots it in headless Chromium against the Tauri-IPC stub, and
// captures the full route × viewport × theme matrix into gitignored shots/.
// Any pageerror / console.error fails the run (exit 1).
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";
import { variants } from "./fixtures.ts";
import { ROWS, THEMES, VIEWPORTS, type MatrixRow, type Theme, type Viewport } from "./matrix.ts";

const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const SHOTS_DIR = join(ROOT, "shots");
const PORT = 4173;
const BASE = `http://localhost:${PORT}`;
const VITE_BIN = join(ROOT, "node_modules", "vite", "bin", "vite.js");

// Local declaration (the project ships without @types/node; declaring
// `process` globally would break vite.config.js's @ts-expect-error).
declare const process: {
  env: Record<string, string | undefined>;
  execPath: string;
  version: string;
  exitCode: number | undefined;
  stdout: { write(chunk: string): unknown };
  exit(code?: number): never;
};

// Resolve the version through @playwright/test's own dependency context —
// pnpm's strict layout doesn't hoist playwright-core to the project root.
const requireFromTestPkg = createRequire(
  ((createRequire(import.meta.url) as any).resolve("@playwright/test") as string).replace(/\/[^/]*$/, "/"),
);
const playwrightVersion = (requireFromTestPkg("playwright/package.json") as { version: string }).version;

interface ShotRecord {
  route: string;
  variant: string;
  viewport: string;
  theme: string;
  file: string;
  bytes: number;
  setup: string;
  readySelector: string;
  notes?: string;
}

/** Startup assertion (§5 hazard): the default variant MUST say the wizard is
 * done, or every non-onboarding shot silently becomes the wizard's screen 1. */
function assertWizardDone(): void {
  const status = variants.default["wizard_status"] as { needed: boolean } | undefined;
  if (!status || status.needed !== false) {
    console.error(
      "FATAL: variants.default.wizard_status.needed !== false — every route would redirect to /onboarding.",
    );
    process.exit(1);
  }
}

function build(): void {
  if (process.env.SHOTS_SKIP_BUILD === "1") return;
  console.log("building static bundle (SHOTS_SKIP_BUILD=1 to skip)…");
  const r = spawnSync(process.execPath, [VITE_BIN, "build"], { cwd: ROOT, stdio: "inherit" });
  if (r.status !== 0) {
    console.error("FATAL: vite build failed");
    process.exit(1);
  }
}

async function startPreview(): Promise<ChildProcess> {
  const child = spawn(process.execPath, [VITE_BIN, "preview", "--port", String(PORT), "--strictPort"], {
    cwd: ROOT,
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout?.on("data", () => {});
  child.stderr?.on("data", (d) => console.error(`[preview] ${d}`.trimEnd()));

  const deadline = Date.now() + 30_000;
  for (;;) {
    if (child.exitCode !== null) {
      console.error("FATAL: vite preview exited before serving");
      process.exit(1);
    }
    try {
      const res = await fetch(BASE);
      if (res.ok) break;
    } catch {
      // not up yet
    }
    if (Date.now() > deadline) {
      console.error("FATAL: vite preview did not come up within 30s");
      child.kill();
      process.exit(1);
    }
    await new Promise((r) => setTimeout(r, 250));
  }

  // Gate step 0: SPA deep links must resolve to the shell (adapter fallback).
  const deep = await fetch(`${BASE}/table/play`);
  const body = await deep.text();
  if (!deep.ok || !body.toLowerCase().includes("<!doctype html")) {
    child.kill();
    console.error(`FATAL: deep link /table/play did not return the SPA shell (status ${deep.status})`);
    process.exit(1);
  }
  return child;
}

async function captureRow(
  browser: import("@playwright/test").Browser,
  row: MatrixRow,
  vp: Viewport,
  theme: Theme,
  errorsByRoute: Map<string, string[]>,
): Promise<ShotRecord> {
  const context = await browser.newContext({
    viewport: { width: vp.width, height: vp.height },
    deviceScaleFactor: vp.deviceScaleFactor,
    isMobile: vp.isMobile,
    hasTouch: vp.hasTouch,
    colorScheme: theme,
  });
  // Init-script order is load-bearing: fixture injector first, stub second —
  // both before any page script.
  await context.addInitScript(
    (fixtures: unknown) => {
      (window as unknown as Record<string, unknown>).__E2E_FIXTURES__ = fixtures;
    },
    structuredClone(variants[row.variant]),
  );
  await context.addInitScript({ path: join(ROOT, "e2e", "tauri-stub.js") });

  const key = `${row.slug} [${vp.name}/${theme}]`;
  const errors: string[] = [];
  errorsByRoute.set(key, errors);
  context.on("page", (page) => {
    page.on("pageerror", (e) => errors.push(`pageerror: ${e}`));
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(`console.error: ${msg.text()}`);
    });
  });

  const page = await context.newPage();
  try {
    await page.goto(BASE + row.url, { waitUntil: "domcontentloaded" });
    await row.setup?.(page);
    if (row.readyFn) {
      await row.readyFn(page);
    } else {
      await page.waitForSelector(row.readySelector, { timeout: 15_000 });
    }
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(300); // settle

    const dir = join(SHOTS_DIR, row.slug);
    mkdirSync(dir, { recursive: true });
    const file = `${vp.name}-${theme}.png`;
    const buf = await page.screenshot({ fullPage: true });
    // Write via Buffer (node:fs) — bytes land identical to page.screenshot's.
    writeFileSync(join(dir, file), buf);
    return {
      route: row.route,
      variant: row.variant,
      viewport: vp.name,
      theme,
      file: `shots/${row.slug}/${file}`,
      bytes: buf.byteLength,
      setup: row.setupNote,
      readySelector: row.readyFn ? "(custom readiness fn)" : row.readySelector,
      ...(row.notes ? { notes: row.notes } : {}),
    };
  } finally {
    await context.close();
  }
}

async function main(): Promise<void> {
  assertWizardDone();
  build();

  rmSync(SHOTS_DIR, { recursive: true, force: true });
  const preview = await startPreview();

  let browser: import("@playwright/test").Browser | null = null;
  const shots: ShotRecord[] = [];
  const errorsByRoute = new Map<string, string[]>();
  try {
    browser = await chromium.launch({
      headless: true,
      args: ["--hide-scrollbars", "--force-color-profile=srgb", "--font-render-hinting=none"],
    });
    for (const row of ROWS) {
      for (const vp of VIEWPORTS) {
        for (const theme of THEMES) {
          process.stdout.write(`  ${row.slug} ${vp.name}/${theme}…`);
          shots.push(await captureRow(browser, row, vp, theme, errorsByRoute));
          console.log(" done");
        }
      }
    }
  } catch (e) {
    console.error(`\nFATAL: ${e}`);
    process.exitCode = 1;
  } finally {
    await browser?.close();
    preview.kill();
  }

  writeFileSync(
    join(SHOTS_DIR, "manifest.json"),
    JSON.stringify(
      {
        generatedBy: "pnpm e2e:shots",
        node: process.version,
        playwright: playwrightVersion,
        routes: shots,
      },
      null,
      2,
    ) + "\n",
  );

  let failed = process.exitCode === 1;
  console.log("\nper-route errors:");
  for (const [key, errors] of errorsByRoute) {
    if (errors.length > 0) {
      failed = true;
      console.error(`  ✗ ${key}`);
      for (const e of errors) console.error(`      ${e}`);
    }
  }
  console.log(`\n${shots.length} PNGs → ${SHOTS_DIR}/ (+ manifest.json)`);
  if (failed) {
    console.error("FAILED — see errors above");
    process.exit(1);
  }
  console.log("OK — zero page/console errors");
}

await main();
