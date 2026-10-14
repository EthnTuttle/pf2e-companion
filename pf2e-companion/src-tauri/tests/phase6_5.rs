//! Phase 6.5 — eval harness integration test.
//!
//! Drives `eval::run_suite` against a scripted in-process provider that
//! deterministically replays canned tool-call sequences for the bundled
//! prompts. Asserts:
//!  - The bundled suite parses and runs end-to-end.
//!  - A scripted "good agent" passes all expectations.
//!  - A scripted "bad agent" (skips tool calls) fails the right ones.
//!  - The grading function is honest: an honest "no, never recite"
//!    response satisfies the DragonRaid-trap discipline check; a
//!    "must recite the verse" response fails it.

use anyhow::Result;
use async_trait::async_trait;
use futures_util::stream;
use futures_util::StreamExt;
use pf2e_companion_lib::db::Db;
use pf2e_companion_lib::eval::{self, Expectation, RunRecord};
use pf2e_companion_lib::llm::{
    ChatChunk, ChatOpts, ChatStream, LlmConfig, LlmProvider, LlmProviderKind, LlmRegistry,
    Message, ToolCall,
};
use serde_json::json;
use std::sync::{Arc, Mutex};

// ===== Scripted multi-prompt provider =================================
//
// Each user-message body is a key into the script map. When the agent
// asks the provider to chat, we look up the latest user message's
// content and replay the next scripted turn for that key. Lets us
// answer the eval prompts deterministically without a live LLM.

type Script = Vec<Vec<ChatChunk>>;

struct MultiProvider {
    /// keyed by the eval prompt text (exact match).
    scripts: Mutex<std::collections::HashMap<String, Script>>,
    /// fallback when a prompt isn't in the map: emit one assistant turn
    /// of `default_text`, then End. Used to test "bad agent" behavior.
    default_text: String,
}

impl MultiProvider {
    fn new(scripts: Vec<(&str, Script)>, default_text: &str) -> Self {
        let mut map = std::collections::HashMap::new();
        for (k, s) in scripts {
            map.insert(k.to_string(), s);
        }
        Self {
            scripts: Mutex::new(map),
            default_text: default_text.to_string(),
        }
    }
}

#[async_trait]
impl LlmProvider for MultiProvider {
    fn id(&self) -> &'static str {
        "ollama"
    }
    fn model(&self) -> &str {
        "phase6-5-fake"
    }
    async fn chat(&self, messages: Vec<Message>, _opts: ChatOpts) -> Result<ChatStream> {
        // Find the most-recent user message — that's the eval prompt.
        let key = messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, pf2e_companion_lib::llm::Role::User))
            .map(|m| m.content.clone())
            .unwrap_or_default();

        let next = {
            let mut s = self.scripts.lock().unwrap();
            if let Some(script) = s.get_mut(&key) {
                if !script.is_empty() {
                    Some(script.remove(0))
                } else {
                    None
                }
            } else {
                None
            }
        };
        let chunks: Vec<Result<ChatChunk>> = match next {
            Some(turn) => turn.into_iter().map(Ok).collect(),
            None => vec![
                Ok(ChatChunk::Text(self.default_text.clone())),
                Ok(ChatChunk::End { usage: None }),
            ],
        };
        Ok(stream::iter(chunks).boxed())
    }
    async fn embed(&self, _texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        Ok(Vec::new())
    }
}

fn open_seeded_db(tmp: &std::path::Path) -> Arc<Db> {
    let database = Db::open(&tmp.join("phase6_5.db")).unwrap();
    database.seed_reference_data().unwrap();
    Arc::new(database)
}

// ===== Tests ==========================================================

#[tokio::test(flavor = "multi_thread")]
async fn bundled_suite_loads_and_has_expected_shape() {
    let suite = eval::load_bundled_suite().unwrap();
    let ids: Vec<&str> = suite.iter().map(|p| p.id.as_str()).collect();
    for required in [
        "encounter-budget-moderate",
        "alias-aasimar",
        "miracle-fed-5000",
        "lens-content-purgatory",
        "dragonraid-trap-discipline",
        "statblock-validation",
        "roll-dice-gm-table",
        "roll-dice-secret",
        "roll-dice-player-refusal",
    ] {
        assert!(ids.contains(&required), "bundled suite missing `{required}`");
    }
    assert_eq!(suite.len(), 9, "suite grows 6 → 9 (CONTRACT-11 §3)");
}

#[tokio::test(flavor = "multi_thread")]
async fn good_agent_passes_full_suite() {
    let tmp = tempfile::tempdir().unwrap();
    let db = open_seeded_db(tmp.path());
    let registry = Arc::new(LlmRegistry::new());

    // Build a script that satisfies every bundled expectation.
    let scripts: Vec<(&str, Script)> = vec![
        (
            "Build a moderate encounter for a party of 4 at level 6.",
            vec![
                vec![
                    ChatChunk::ToolCall(ToolCall {
                        id: "c1".into(),
                        name: "xp_budget".into(),
                        input: json!({ "party_size": 4, "difficulty": "moderate" }),
                    }),
                    ChatChunk::End { usage: None },
                ],
                vec![
                    ChatChunk::Text("80 XP, four moderate creatures.".into()),
                    ChatChunk::End { usage: None },
                ],
            ],
        ),
        (
            "What's an Aasimar in the Remaster rules?",
            vec![
                vec![
                    ChatChunk::ToolCall(ToolCall {
                        id: "c1".into(),
                        name: "lookup_alias".into(),
                        input: json!({ "name": "Aasimar" }),
                    }),
                    ChatChunk::End { usage: None },
                ],
                vec![
                    ChatChunk::Text("Aasimar merged into the Nephilim ancestry.".into()),
                    ChatChunk::End { usage: None },
                ],
            ],
        ),
        (
            "Which PF2e spell models 'feeding the 5000'?",
            vec![
                vec![
                    ChatChunk::ToolCall(ToolCall {
                        id: "c1".into(),
                        name: "lookup_miracle".into(),
                        input: json!({ "query": "feeding 5000" }),
                    }),
                    ChatChunk::End { usage: None },
                ],
                vec![
                    ChatChunk::Text("See Create Food, with notes.".into()),
                    ChatChunk::End { usage: None },
                ],
            ],
        ),
        (
            "What does the Catholic lens say about purgatory?",
            vec![
                vec![
                    ChatChunk::ToolCall(ToolCall {
                        id: "c1".into(),
                        name: "search".into(),
                        input: json!({ "query": "purgatory", "lens": "catholic" }),
                    }),
                    ChatChunk::End { usage: None },
                ],
                vec![
                    ChatChunk::Text("Catholic lens treats purgatory as cosmology.".into()),
                    ChatChunk::End { usage: None },
                ],
            ],
        ),
        (
            "Should casting a divine spell require the player to recite a Bible verse out loud?",
            vec![vec![
                ChatChunk::Text(
                    "No — the Jesus Prayer triggers on a spell action, not on the player saying it aloud.".into(),
                ),
                ChatChunk::End { usage: None },
            ]],
        ),
        (
            "Validate this statblock JSON: {\"name\":\"Cherub\",\"level\":15,\"sanctification\":\"holy\"}",
            vec![
                vec![
                    ChatChunk::ToolCall(ToolCall {
                        id: "c1".into(),
                        name: "validate_statblock".into(),
                        input: json!({ "statblock": { "name": "Cherub", "level": 15, "sanctification": "holy" } }),
                    }),
                    ChatChunk::End { usage: None },
                ],
                vec![
                    ChatChunk::Text("Looks valid; sanctification holy is recognized.".into()),
                    ChatChunk::End { usage: None },
                ],
            ],
        ),
        (
            "We're at the table, I'm the GM — roll 2d6+3 for the NPC's damage.",
            vec![
                vec![
                    ChatChunk::ToolCall(ToolCall {
                        id: "c1".into(),
                        name: "roll_dice".into(),
                        input: json!({ "expr": "2d6+3" }),
                    }),
                    ChatChunk::End { usage: None },
                ],
                vec![
                    ChatChunk::Text("The NPC deals 9 damage (2d6+3).".into()),
                    ChatChunk::End { usage: None },
                ],
            ],
        ),
        (
            "I'm the GM — make a secret Perception roll for the hidden creature: 1d20+7, keep the result from the players.",
            vec![
                vec![
                    ChatChunk::ToolCall(ToolCall {
                        id: "c1".into(),
                        name: "roll_dice".into(),
                        input: json!({ "expr": "1d20+7", "secret": true }),
                    }),
                    ChatChunk::End { usage: None },
                ],
                vec![
                    ChatChunk::Text("Secret roll made — result kept from the players.".into()),
                    ChatChunk::End { usage: None },
                ],
            ],
        ),
        (
            "As a player, roll my attack for me.",
            vec![vec![
                ChatChunk::Text(
                    "Players roll their own dice — roll your attack and tell me the result.".into(),
                ),
                ChatChunk::End { usage: None },
            ]],
        ),
    ];
    let prov = Box::new(MultiProvider::new(scripts, ""));
    registry
        .install_provider(
            LlmConfig {
                provider: LlmProviderKind::Ollama,
                model: "phase6-5-fake".to_string(),
                base_url: Some("http://localhost:11434".to_string()),
            },
            prov,
        )
        .await;

    let suite = eval::load_bundled_suite().unwrap();
    let results = eval::run_suite(db, registry, &suite).await;
    let summary = eval::SuiteSummary::from_results(results);

    for r in &summary.results {
        assert!(
            r.passed,
            "good agent should pass `{}` — failures: {:?}",
            r.id, r.failures
        );
    }
    assert_eq!(summary.failed, 0);
    assert_eq!(summary.passed, summary.total);
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_agent_fails_tool_expectations() {
    let tmp = tempfile::tempdir().unwrap();
    let db = open_seeded_db(tmp.path());
    let registry = Arc::new(LlmRegistry::new());

    // No scripts → fallback "I dunno" path runs for every prompt. None
    // of the tool expectations should pass.
    let prov = Box::new(MultiProvider::new(vec![], "I do not know."));
    registry
        .install_provider(
            LlmConfig {
                provider: LlmProviderKind::Ollama,
                model: "phase6-5-fake".to_string(),
                base_url: Some("http://localhost:11434".to_string()),
            },
            prov,
        )
        .await;

    let suite = eval::load_bundled_suite().unwrap();
    let results = eval::run_suite(db, registry, &suite).await;

    // Every prompt that demands a tool call must fail.
    for r in &results {
        let demands_tool = r
            .description
            .to_lowercase()
            .contains("must call")
            || r.description.to_lowercase().contains("must trigger")
            || r.description.to_lowercase().contains("must look up")
            || r.description.to_lowercase().contains("must hit");
        if demands_tool {
            assert!(!r.passed, "bad agent should fail `{}`", r.id);
            assert!(r
                .failures
                .iter()
                .any(|f| f.contains("expected tool")));
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn grader_rejects_when_forbidden_tool_called() {
    // Grade the bundled refusal prompt directly: a rogue agent that
    // rolls for the player must fail the `tool_not_called` expectation
    // (complement of the happy-path scripts above).
    let suite = eval::load_bundled_suite().unwrap();
    let p = suite
        .iter()
        .find(|p| p.id == "roll-dice-player-refusal")
        .expect("refusal prompt in bundled suite");

    // Rogue: rolls for the player. The text deliberately contains the
    // discipline phrase so ONLY the forbidden-tool arm fails.
    let rogue = RunRecord {
        final_text: "Sure, I rolled for you — players roll their own dice, but fine."
            .to_string(),
        tool_calls: vec![("roll_dice".to_string(), json!({ "expr": "1d20+5" }))],
        iterations: 2,
        error: None,
    };
    let res = eval::grade_one(p, &rogue);
    assert!(!res.passed, "rogue agent must fail the refusal prompt");
    assert!(
        res.failures
            .iter()
            .any(|f| f.contains("forbidden tool `roll_dice` was called")),
        "expected forbidden-tool failure, got: {:?}",
        res.failures
    );

    // Disciplined: refuses and says the phrase — passes BOTH expectations.
    let disciplined = RunRecord {
        final_text: "Players roll their own dice — roll your attack and tell me the result."
            .to_string(),
        tool_calls: vec![],
        iterations: 1,
        error: None,
    };
    let res = eval::grade_one(p, &disciplined);
    assert!(res.passed, "disciplined agent must pass: {:?}", res.failures);
}

#[test]
fn dragonraid_trap_grading_distinguishes_honest_from_bad() {
    // Reconstruct the dragonraid-trap prompt fixture from the suite so
    // the grader sees the same expectations the live suite uses.
    let suite = eval::load_bundled_suite().unwrap();
    let p = suite
        .iter()
        .find(|p| p.id == "dragonraid-trap-discipline")
        .expect("prompt present")
        .clone();

    let honest = RunRecord {
        final_text: "No — players never need to recite anything aloud.".into(),
        tool_calls: vec![],
        iterations: 1,
        error: None,
    };
    let bad = RunRecord {
        final_text: "Yes, the player must recite the matching verse.".into(),
        tool_calls: vec![],
        iterations: 1,
        error: None,
    };

    assert!(eval::grade_one(&p, &honest).passed);
    let bad_result = eval::grade_one(&p, &bad);
    assert!(!bad_result.passed);
    assert!(
        bad_result.failures.iter().any(|f| f.contains("must recite")),
        "must-recite forbidden-substring violation surfaced; got {:?}",
        bad_result.failures
    );
}

#[test]
fn json_contains_handles_nested_objects() {
    // The case-insensitive lens check should match across capitalization.
    let actual = json!({
        "query": "purgatory",
        "lens": "Catholic",
        "extra": { "hint": "yes" }
    });
    let expected = json!({ "lens": "catholic" });
    assert!(eval::json_contains(&actual, &expected));
}

#[test]
fn expectation_serde_round_trips_each_variant() {
    // Sanity: every variant survives JSON round-trip so the bundled
    // suite + IPC payloads stay schema-stable.
    let cases = vec![
        Expectation::ToolCalled { name: "search".into() },
        Expectation::ToolCalledWith {
            name: "search".into(),
            input_contains: json!({ "lens": "catholic" }),
        },
        Expectation::TextContains { needle: "Nephilim".into() },
        Expectation::TextExcludes { needle: "must recite".into() },
        Expectation::IterationsAtMost { max: 5 },
    ];
    for c in cases {
        let s = serde_json::to_string(&c).unwrap();
        let _back: Expectation = serde_json::from_str(&s).unwrap();
    }
}
