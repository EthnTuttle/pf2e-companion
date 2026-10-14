// e2e/matrix.ts — the route × viewport × theme table + per-route setup.
//
// 20 rows × 2 viewports × 2 themes = 80 PNGs. Every ready selector is
// route-specific CONTENT (hit rows, statblocks, "Step N of 3 · …"), never
// layout chrome — a wrong-variant redirect must fail the row, not pass it.
import type { Page } from "@playwright/test";
import { SNAPSHOT, variants } from "./fixtures.ts";

export type VariantName = keyof typeof variants;

export interface Viewport {
  name: "desktop" | "mobile";
  width: number;
  height: number;
  deviceScaleFactor: number;
  isMobile: boolean;
  hasTouch: boolean;
}

export const VIEWPORTS: Viewport[] = [
  { name: "desktop", width: 1280, height: 800, deviceScaleFactor: 1, isMobile: false, hasTouch: false },
  { name: "mobile", width: 390, height: 844, deviceScaleFactor: 2, isMobile: true, hasTouch: true },
];

export const THEMES = ["light", "dark"] as const;
export type Theme = (typeof THEMES)[number];

export interface MatrixRow {
  /** Human-readable route for the manifest. */
  route: string;
  /** URL to goto (already encoded where needed). */
  url: string;
  /** shots/ subdirectory name. */
  slug: string;
  variant: VariantName;
  /** Extra driving beyond goto (clicks, fixture emits). */
  setup?: (page: Page) => Promise<void>;
  readySelector: string;
  /** Custom readiness check (chat-reply text stability). */
  readyFn?: (page: Page) => Promise<void>;
  setupNote: string;
  notes?: string;
}

const ABOLETH_ID = "c-runes:creature/aboleth-tidebreaker";
const ABOLETH_URL = encodeURIComponent(ABOLETH_ID);

/** Emit the fixture snapshot through the page's real listen → applyPatch path. */
async function emitSnapshot(page: Page): Promise<void> {
  await page.waitForFunction(
    () => (window as unknown as Record<string, (e: string) => number>).__e2eListenerCount("session:patch") > 0,
  );
  await page.evaluate((snapshot) => {
    (window as unknown as Record<string, (e: string, p: unknown) => void>).__e2eEmit("session:patch", {
      session_id: "e2e-session",
      kind: "snapshot",
      snapshot,
    });
  }, SNAPSHOT);
}

export const ROWS: MatrixRow[] = [
  {
    route: "/", url: "/", slug: "root", variant: "default",
    readySelector: ".hit-link", setupNote: "direct goto",
  },
  {
    route: "/encounter", url: "/encounter", slug: "encounter", variant: "default",
    readySelector: ".result .num", setupNote: "direct goto",
  },
  {
    route: "/vault", url: "/vault", slug: "vault", variant: "default",
    readySelector: ".entities .hits li a.hit", setupNote: "direct goto",
  },
  {
    route: "/vault/new", url: "/vault/new", slug: "vault-new", variant: "default",
    readySelector: "#title-input", setupNote: "direct goto",
  },
  {
    route: `/vault/${ABOLETH_ID}`, url: `/vault/${ABOLETH_URL}`, slug: "vault-aboleth", variant: "default",
    readySelector: "form.editor ul.rels li", setupNote: "fixture entity (vault-sourced, editable)",
  },
  {
    route: `/entity/${ABOLETH_ID}`, url: `/entity/${ABOLETH_URL}`, slug: "entity-aboleth", variant: "default",
    readySelector: ".statblock", setupNote: "fixture entity (statblock + body)",
  },
  {
    route: "/aliases", url: "/aliases", slug: "aliases", variant: "default",
    readySelector: ".hits li .rename", setupNote: "direct goto",
  },
  {
    route: "/miracles", url: "/miracles", slug: "miracles", variant: "default",
    readySelector: ".hits li header", setupNote: "direct goto",
  },
  {
    route: "/table", url: "/table", slug: "table-lobby", variant: "default",
    readySelector: ".saved-list li", setupNote: "direct goto (lobby)",
  },
  {
    route: "/table/play (GM)", url: "/table", slug: "table-play-gm", variant: "default",
    readySelector: ".chat-log .msg",
    setupNote: 'lobby: fill name "GM", click Host Session → auto-goto /table/play, then emit fixture snapshot',
    notes: "fixture-snapshot",
    async setup(page) {
      await page.fill('input[placeholder="Your name"]', "GM");
      await page.click('button:has-text("Host Session")');
      await page.waitForURL("**/table/play");
      await emitSnapshot(page);
    },
  },
  {
    route: "/table/play (player)", url: "/table/play?name=Rook", slug: "table-play-player", variant: "default",
    readySelector: ".chat-log .msg",
    setupNote: 'direct goto ?name=Rook (IPC transport via stub), then emit fixture snapshot',
    notes: "fixture-snapshot",
    async setup(page) {
      await page.waitForSelector(".play-tabs");
      await emitSnapshot(page);
    },
  },
  {
    route: "/chat", url: "/chat", slug: "chat", variant: "default",
    readySelector: '.tagline a[href="/settings/llm"]', setupNote: "direct goto (unconfigured-LLM notice)",
  },
  {
    route: "/settings", url: "/settings", slug: "settings", variant: "default",
    readySelector: "section.card h2", setupNote: "direct goto (diagnostics card)",
  },
  {
    route: "/settings/llm", url: "/settings/llm", slug: "settings-llm", variant: "default",
    readySelector: "section.rag .rag-status", setupNote: "direct goto (RAG stats card)",
  },
  {
    route: "/settings/eval", url: "/settings/eval", slug: "settings-eval", variant: "default",
    readySelector: "ul.prompts li.prompt", setupNote: "direct goto (suite list)",
  },
  {
    route: "/about", url: "/about", slug: "about", variant: "default",
    readySelector: "article.prose", setupNote: "direct goto",
  },
  {
    route: "/onboarding screen 1 (Vault)", url: "/onboarding", slug: "onboarding-step1", variant: "wizardFresh",
    readySelector: "text=Step 1 of 3 · Vault",
    setupNote: "wizardFresh variant; direct goto (redirect suppressed inside /onboarding)",
  },
  {
    route: "/onboarding screen 2 (Lens)", url: "/onboarding", slug: "onboarding-step2", variant: "wizardFresh",
    readySelector: "text=Step 2 of 3 · Lens",
    setupNote: "goto, then real click on Next (wizardState.step 0→1)",
    async setup(page) {
      await page.click('button:has-text("Next")');
    },
  },
  {
    route: "/onboarding screen 3 (LLM)", url: "/onboarding", slug: "onboarding-step3", variant: "wizardFresh",
    readySelector: "text=Step 3 of 3 · LLM",
    setupNote: 'goto, click Next twice, then click the "Anthropic" radio to show the API-key field',
    async setup(page) {
      await page.click('button:has-text("Next")');
      await page.waitForSelector("text=Step 2 of 3 · Lens");
      await page.click('button:has-text("Next")');
      await page.waitForSelector("text=Step 3 of 3 · LLM");
      await page.click('label.option:has-text("Anthropic") input');
    },
  },
  {
    route: "/chat (scripted reply)", url: "/chat", slug: "chat-reply", variant: "chatConfigured",
    readySelector: ".msg[data-role=assistant] .content",
    setupNote: 'chatConfigured variant; fill draft, click Send → llm_chat stub returns e2e-chat-1 → replay flushes through the page\'s real llm:token listener',
    notes: "stubbed-backend: llm:token replay",
    async setup(page) {
      await page.fill(".composer textarea", "Size an aboleth encounter for my party of four 5th-level heroes.");
      await page.click('.composer button.primary');
    },
    async readyFn(page) {
      // Wait for the done event: assistant text complete + tool trace rendered.
      await page.waitForFunction(() => {
        const el = document.querySelector('.msg[data-role="assistant"] .content');
        return !!el && (el.textContent ?? "").includes("diving alone");
      });
      await page.waitForSelector(".traces .trace");
    },
  },
];
