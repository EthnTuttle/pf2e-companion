// Fixtures for the settings / eval surfaces (PLAN §3.3, §4 S10–S12 land in
// 12.1). fixtureEvalSuite is 3 prompts; fixtureSuiteSummary is a 2-pass /
// 1-fail run whose failure string S11 asserts on.
import type {
  EvalPrompt,
  LlmStatus,
  RagIndexStats,
  SuiteSummary,
} from "$lib/ipc";

export const fixtureEvalSuite: EvalPrompt[] = [
  {
    id: "xp-budget",
    description: "Asks for a severe XP budget",
    prompt: "What is the XP budget for a severe encounter?",
    lens: null,
    system_extra: null,
    expectations: [{ kind: "tool_called", name: "xp_budget" }],
  },
  {
    id: "alias-resolve",
    description: "Resolves a legacy spell name",
    prompt: "What is Magic Missile called in the Remaster?",
    lens: null,
    system_extra: null,
    expectations: [{ kind: "tool_called", name: "lookup_alias" }],
  },
  {
    id: "roll-dice",
    description: "Rolls dice via tool",
    prompt: "Roll initiative for me (d20).",
    lens: null,
    system_extra: null,
    expectations: [{ kind: "tool_called", name: "roll_dice" }],
  },
];

export const fixtureSuiteSummary: SuiteSummary = {
  total: 3,
  passed: 2,
  failed: 1,
  results: [
    {
      id: "xp-budget",
      description: "Asks for a severe XP budget",
      passed: true,
      failures: [],
      final_text: "A severe encounter for 4 PCs is 120 XP.",
      tool_calls: [
        { name: "xp_budget", input: { partySize: 4, difficulty: "severe" } },
      ],
      iterations: 1,
      error: null,
    },
    {
      id: "alias-resolve",
      description: "Resolves a legacy spell name",
      passed: true,
      failures: [],
      final_text: "It's now called Force Barrage.",
      tool_calls: [{ name: "lookup_alias", input: { name: "Magic Missile" } }],
      iterations: 1,
      error: null,
    },
    {
      id: "roll-dice",
      description: "Rolls dice via tool",
      passed: false,
      failures: ["expected tool_called roll_dice"],
      final_text: "You rolled a 14!",
      tool_calls: [],
      iterations: 1,
      error: null,
    },
  ],
};

/** S10 stubs the RAG index empty (LLM-off, nothing indexed yet). */
export const fixtureRagStatsEmpty: RagIndexStats = {
  indexed: false,
  entities: 0,
  chunks: 0,
  provider: null,
  model: null,
};

/**
 * S11 only: /settings/eval hard-disables "Run suite" while llm_status is
 * unconfigured, so the scenario needs a configured status to reach the run.
 * eval_run itself stays fixture-stubbed — no LLM is ever contacted.
 */
export const fixtureLlmConfigured: LlmStatus = {
  configured: true,
  provider: "ollama",
  model: "qwen3:4b",
  key_present: false,
};
