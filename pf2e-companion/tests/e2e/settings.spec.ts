// Group C — settings / LLM-off + eval (PLAN §4, S10–S12).
// Every IPC call these routes make is stubbed via installHarness; assertions
// target the real rendered DOM of src/routes/settings + src/routes/chat.
//
// §4→reality adaptation (recorded in PLAN §9, 12.1 note): S11's "click Run"
// is unreachable under the default LLM-off fixture — /settings/eval
// hard-disables the button while llm_status.configured is false and runSuite
// early-returns. S11 therefore stubs llm_status → fixtureLlmConfigured; the
// run itself remains fully stubbed (eval_run returns the fixture summary).
import { expect, test } from "@playwright/test";
import { installHarness, invocations, shot } from "./helpers/harness";
import {
  fixtureEvalSuite,
  fixtureLlmConfigured,
  fixtureRagStatsEmpty,
  fixtureSuiteSummary,
} from "./fixtures/settings";

test("S10 llm-off", async ({ page }) => {
  await installHarness(page, {
    rag_index_stats: { ok: fixtureRagStatsEmpty },
  });
  await page.goto("/settings/llm");

  await expect(
    page.getByRole("heading", { level: 1, name: "LLM settings" }),
  ).toBeVisible();
  // Not-configured guidance: disclaimer card, the RAG gate copy next to the
  // (disabled) index action, empty RAG index status, and no
  // "Currently configured" status line.
  await expect(
    page.getByRole("heading", { name: "Before you enable" }),
  ).toBeVisible();
  await expect(page.getByText("Configure a provider above first.")).toBeVisible();
  await expect(
    page.getByText("Not indexed yet — search uses FTS only."),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Index content" }),
  ).toBeDisabled();
  await expect(page.locator("p.status")).toHaveCount(0);
  await shot(page, "settings/01-llm-off");

  // No form submit happened (and none can happen silently): llm_configure
  // was never invoked.
  expect(
    (await invocations(page)).filter((r) => r.cmd === "llm_configure"),
  ).toHaveLength(0);
});

test("S11 eval-results", async ({ page }) => {
  // llm_status override reason: with the base LLM-off stub the "Run suite"
  // button is disabled and the run can't start. The summary below is a
  // fixture; nothing real talks to an LLM.
  await installHarness(page, {
    llm_status: { ok: fixtureLlmConfigured },
    eval_load_suite: { ok: fixtureEvalSuite },
    eval_run: { ok: fixtureSuiteSummary },
  });
  await page.goto("/settings/eval");

  await expect(
    page.getByRole("heading", { level: 1, name: "Eval harness" }),
  ).toBeVisible();
  const prompts = page.locator("ul.prompts li.prompt");
  await expect(prompts).toHaveCount(fixtureEvalSuite.length);
  await expect(page.locator('li.prompt[data-status="pending"]')).toHaveCount(3);

  await page.getByRole("button", { name: "Run suite" }).click();

  // 2/3 badge…
  await expect(page.locator(".summary")).toContainText("2 / 3");
  await expect(page.locator(".summary")).toContainText("1 failed");
  // …two passing prompts…
  await expect(page.locator('li.prompt[data-status="ok"]')).toHaveCount(2);
  // …and the one failing prompt named, badged, and explained.
  const failed = page.locator('li.prompt[data-status="fail"]');
  await expect(failed).toHaveCount(1);
  await expect(failed.locator(".pid")).toHaveText("roll-dice");
  await expect(failed.locator(".status")).toHaveText("FAIL");
  await expect(page.locator(".failures li")).toHaveText(
    "expected tool_called roll_dice",
  );
  await shot(page, "settings/02-eval-results");

  expect(
    (await invocations(page)).filter((r) => r.cmd === "eval_run"),
  ).toHaveLength(1);
});

test("S12 chat-gated", async ({ page }) => {
  await installHarness(page);
  await page.goto("/chat");

  await expect(
    page.getByRole("heading", { level: 1, name: "Chat" }),
  ).toBeVisible();
  // Gate copy: tagline points at /settings/llm and the composer is disabled
  // with "Configure /settings/llm first" as its placeholder.
  await expect(
    page.getByRole("link", { name: "Configure a provider →" }),
  ).toBeVisible();
  const composer = page.locator("form.composer textarea");
  await expect(composer).toBeDisabled();
  await expect(composer).toHaveAttribute(
    "placeholder",
    "Configure /settings/llm first",
  );
  await expect(
    page.getByRole("button", { name: "Send", exact: true }),
  ).toBeDisabled();
  await shot(page, "settings/03-chat-gated");

  // No llm_chat invocation is possible while the gate holds.
  expect(
    (await invocations(page)).filter((r) => r.cmd === "llm_chat"),
  ).toHaveLength(0);
});
