//! Phase 6.5: canon-faithfulness + statblock-validity eval harness.
//!
//! Runs the agent loop against a fixed prompt suite, capturing every
//! `AgentEvent`, then grades each run with a tiny assertion DSL.
//!
//! Two surfaces:
//! - **`run_suite`** — execute a `Suite` end-to-end against the
//!   currently configured provider, return one `RunResult` per prompt.
//! - **`grade`** — pure function over a `RunRecord` + `Vec<Expectation>`,
//!   testable without a live provider (the real value lives here: when
//!   you change a system prompt, this layer catches the regression).
//!
//! The bundled suite (`data/eval/prompts.json`) is small on purpose. Add
//! prompts as you tune; the assertion vocabulary stays tight so each
//! eval reads in one screen.

use crate::db::Db;
use crate::llm::{LlmRegistry, Message, Role};
use crate::llm_tools::{self, AgentEvent};
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

/// Bundled prompt suite. Loaded at process start (eagerly cheap;
/// frontend can also pull this for display). Hand-edit
/// `data/eval/prompts.json` to add or revise.
const BUNDLED_SUITE: &str = include_str!("../data/eval/prompts.json");

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Prompt {
    pub id: String,
    pub description: String,
    pub prompt: String,
    #[serde(default)]
    pub lens: Option<String>,
    /// Free-form addition appended to the system prompt for this run.
    #[serde(default)]
    pub system_extra: Option<String>,
    pub expectations: Vec<Expectation>,
}

/// The assertion vocabulary. Kept tight; if you need a new one, add a
/// variant + a match arm in `grade_one`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Expectation {
    /// At least one ToolCall event named `name`.
    ToolCalled { name: String },
    /// At least one ToolCall event whose name matches AND whose input
    /// JSON contains every key/value pair in `input_contains` (string,
    /// number, and bool comparison; nested objects compared recursively
    /// by superset).
    ToolCalledWith {
        name: String,
        input_contains: Value,
    },
    /// Final assistant text contains `needle` (case-insensitive substring).
    TextContains { needle: String },
    /// Final assistant text does NOT contain `needle`.
    TextExcludes { needle: String },
    /// Iteration count is at most `max`.
    IterationsAtMost { max: usize },
    /// No ToolCall event named `name` may occur in the run.
    ToolNotCalled { name: String },
}

#[derive(Debug, Clone, Default)]
pub struct RunRecord {
    pub final_text: String,
    pub tool_calls: Vec<(String, Value)>,
    pub iterations: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunResult {
    pub id: String,
    pub description: String,
    pub passed: bool,
    pub failures: Vec<String>,
    pub final_text: String,
    pub tool_calls: Vec<ToolCallSummary>,
    pub iterations: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCallSummary {
    pub name: String,
    pub input: Value,
}

/// Load the bundled prompt suite.
pub fn load_bundled_suite() -> Result<Vec<Prompt>> {
    serde_json::from_str(BUNDLED_SUITE).context("parse bundled eval suite")
}

/// Run every prompt in the suite. Each prompt is its own conversation;
/// no context bleeds between them. Failures inside a single prompt
/// (provider error, tool error, etc.) record the error but do not stop
/// the suite — we want a complete report.
pub async fn run_suite(
    db: Arc<Db>,
    registry: Arc<LlmRegistry>,
    prompts: &[Prompt],
) -> Vec<RunResult> {
    let mut out = Vec::with_capacity(prompts.len());
    for p in prompts {
        out.push(run_one(db.clone(), registry.clone(), p).await);
    }
    out
}

async fn run_one(db: Arc<Db>, registry: Arc<LlmRegistry>, p: &Prompt) -> RunResult {
    let messages = vec![Message::new(Role::User, p.prompt.clone())];
    let system = build_system_for(p);
    let mut rx = llm_tools::run_agent(
        db,
        registry,
        messages,
        system,
        false,
        Some(0.2),
        Some(1024),
    );
    let record = drain_run(&mut rx).await;
    grade_one(p, &record)
}

fn build_system_for(p: &Prompt) -> String {
    let lens = p.lens.as_deref().unwrap_or("lewisian");
    let mut sys = format!(
        "You are an evaluation harness running against a PF2e Remaster + \
         Christian Biblical worldview reference assistant. Active lens: {lens}.\n\
         \n\
         Available tools: xp_budget, validate_statblock, lookup_alias, \
         lookup_miracle, search, roll_dice. Use them when the question requires \
         lookup, validation, or computation rather than guessing.\n\
         \n\
         Discipline rules:\n\
         - Do NOT require real-world prayer, recitation, or piety as a \
           mechanical input. Theology is content, not a rules input.\n\
         - For rules-touching answers, suggest verifying against canonical \
           PF2e Remaster sources.\n\
         - Table discipline: players roll their own dice. Only the GM \
           hosting a table session may ask you to roll for them; if a \
           player asks, tell them to roll their own dice.\n",
    );
    if let Some(extra) = &p.system_extra {
        sys.push('\n');
        sys.push_str(extra);
    }
    sys
}

/// Drain an agent's event stream into a `RunRecord`. Public so tests
/// (`tests/phase6_5.rs`) can drive a scripted provider and grade
/// without a live LLM.
pub async fn drain_run(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<Result<AgentEvent>>,
) -> RunRecord {
    let mut rec = RunRecord::default();
    while let Some(item) = rx.recv().await {
        match item {
            Ok(AgentEvent::Text(s)) => rec.final_text.push_str(&s),
            Ok(AgentEvent::ToolStart { name, input, .. }) => {
                rec.tool_calls.push((name, input));
            }
            Ok(AgentEvent::ToolResult { .. }) => {
                // Result is consumed by the agent; the eval grades on
                // the call itself, not the response shape.
            }
            Ok(AgentEvent::End { iterations, .. }) => {
                rec.iterations = iterations;
                break;
            }
            Err(e) => {
                rec.error = Some(e.to_string());
                break;
            }
        }
    }
    rec
}

/// Grade a single record against its prompt's expectations. Pure
/// function over the record — no I/O.
pub fn grade_one(p: &Prompt, record: &RunRecord) -> RunResult {
    let mut failures: Vec<String> = Vec::new();
    if let Some(e) = &record.error {
        failures.push(format!("agent error: {e}"));
    }
    for exp in &p.expectations {
        if let Err(reason) = check_expectation(exp, record) {
            failures.push(reason);
        }
    }
    RunResult {
        id: p.id.clone(),
        description: p.description.clone(),
        passed: failures.is_empty(),
        failures,
        final_text: record.final_text.clone(),
        tool_calls: record
            .tool_calls
            .iter()
            .map(|(name, input)| ToolCallSummary {
                name: name.clone(),
                input: input.clone(),
            })
            .collect(),
        iterations: record.iterations,
        error: record.error.clone(),
    }
}

fn check_expectation(exp: &Expectation, rec: &RunRecord) -> Result<(), String> {
    match exp {
        Expectation::ToolCalled { name } => {
            if rec.tool_calls.iter().any(|(n, _)| n == name) {
                Ok(())
            } else {
                Err(format!("expected tool `{name}` to be called"))
            }
        }
        Expectation::ToolCalledWith {
            name,
            input_contains,
        } => {
            let any_match = rec
                .tool_calls
                .iter()
                .filter(|(n, _)| n == name)
                .any(|(_, input)| json_contains(input, input_contains));
            if any_match {
                Ok(())
            } else {
                Err(format!(
                    "expected tool `{name}` to be called with input containing {input_contains}"
                ))
            }
        }
        Expectation::TextContains { needle } => {
            if rec.final_text.to_lowercase().contains(&needle.to_lowercase()) {
                Ok(())
            } else {
                Err(format!("text missing expected substring `{needle}`"))
            }
        }
        Expectation::TextExcludes { needle } => {
            if !rec.final_text.to_lowercase().contains(&needle.to_lowercase()) {
                Ok(())
            } else {
                Err(format!("text contains forbidden substring `{needle}`"))
            }
        }
        Expectation::IterationsAtMost { max } => {
            if rec.iterations <= *max {
                Ok(())
            } else {
                Err(format!(
                    "iterations {} exceeded cap {max}",
                    rec.iterations
                ))
            }
        }
        Expectation::ToolNotCalled { name } => {
            if rec.tool_calls.iter().any(|t| t.0 == *name) {
                Err(format!("forbidden tool `{name}` was called"))
            } else {
                Ok(())
            }
        }
    }
}

/// Recursive superset check: every key in `expected` exists in `actual`
/// with an equal scalar value or recursively-matching object/array.
/// Numbers compare via JSON equality (so 4 == 4.0 == 4); strings use
/// case-insensitive equality (lens id "Catholic" matches "catholic").
pub fn json_contains(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => e.iter().all(|(k, v)| {
            a.get(k).map(|av| json_contains(av, v)).unwrap_or(false)
        }),
        (Value::Array(a), Value::Array(e)) => {
            // Each expected element must match SOME actual element
            // (subset semantics).
            e.iter().all(|ev| a.iter().any(|av| json_contains(av, ev)))
        }
        (Value::String(a), Value::String(e)) => a.eq_ignore_ascii_case(e),
        (Value::Number(a), Value::Number(e)) => {
            a.as_f64().zip(e.as_f64()).map(|(a, e)| a == e).unwrap_or(false)
        }
        (Value::Bool(a), Value::Bool(e)) => a == e,
        (Value::Null, Value::Null) => true,
        _ => false,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SuiteSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub results: Vec<RunResult>,
}

impl SuiteSummary {
    pub fn from_results(results: Vec<RunResult>) -> Self {
        let total = results.len();
        let passed = results.iter().filter(|r| r.passed).count();
        Self {
            total,
            passed,
            failed: total - passed,
            results,
        }
    }
}

/// Convenience used by the IPC handler.
pub async fn run_bundled(
    db: Arc<Db>,
    registry: Arc<LlmRegistry>,
) -> Result<SuiteSummary> {
    let suite = load_bundled_suite()?;
    if suite.is_empty() {
        return Err(anyhow!("bundled suite is empty"));
    }
    let results = run_suite(db, registry, &suite).await;
    Ok(SuiteSummary::from_results(results))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rec(text: &str, calls: &[(&str, Value)], iterations: usize) -> RunRecord {
        RunRecord {
            final_text: text.to_string(),
            tool_calls: calls
                .iter()
                .map(|(n, v)| (n.to_string(), v.clone()))
                .collect(),
            iterations,
            error: None,
        }
    }

    #[test]
    fn json_contains_matches_subset_object() {
        let a = json!({ "party_size": 4, "difficulty": "moderate", "extra": 1 });
        let e = json!({ "party_size": 4, "difficulty": "moderate" });
        assert!(json_contains(&a, &e));
    }

    #[test]
    fn json_contains_string_is_case_insensitive() {
        let a = json!({ "lens": "Catholic" });
        let e = json!({ "lens": "catholic" });
        assert!(json_contains(&a, &e));
    }

    #[test]
    fn json_contains_rejects_missing_key() {
        let a = json!({ "difficulty": "moderate" });
        let e = json!({ "party_size": 4 });
        assert!(!json_contains(&a, &e));
    }

    #[test]
    fn grade_passes_when_tool_called() {
        let p = Prompt {
            id: "x".into(),
            description: String::new(),
            prompt: String::new(),
            lens: None,
            system_extra: None,
            expectations: vec![Expectation::ToolCalled {
                name: "xp_budget".into(),
            }],
        };
        let r = rec("ok", &[("xp_budget", json!({}))], 1);
        let res = grade_one(&p, &r);
        assert!(res.passed);
        assert!(res.failures.is_empty());
    }

    #[test]
    fn grade_fails_when_tool_missing() {
        let p = Prompt {
            id: "x".into(),
            description: String::new(),
            prompt: String::new(),
            lens: None,
            system_extra: None,
            expectations: vec![Expectation::ToolCalled {
                name: "search".into(),
            }],
        };
        let r = rec("ok", &[("xp_budget", json!({}))], 1);
        let res = grade_one(&p, &r);
        assert!(!res.passed);
        assert!(res.failures[0].contains("search"));
    }

    #[test]
    fn tool_not_called_passes_when_absent() {
        let p = Prompt {
            id: "x".into(),
            description: String::new(),
            prompt: String::new(),
            lens: None,
            system_extra: None,
            expectations: vec![Expectation::ToolNotCalled {
                name: "roll_dice".into(),
            }],
        };
        // Other tools are fine — only the forbidden name matters.
        let r = rec("players roll their own", &[("xp_budget", json!({}))], 1);
        let res = grade_one(&p, &r);
        assert!(res.passed);
        assert!(res.failures.is_empty());
    }

    #[test]
    fn tool_not_called_fails_when_present() {
        let p = Prompt {
            id: "x".into(),
            description: String::new(),
            prompt: String::new(),
            lens: None,
            system_extra: None,
            expectations: vec![Expectation::ToolNotCalled {
                name: "roll_dice".into(),
            }],
        };
        let r = rec("rolled", &[("roll_dice", json!({ "expr": "1d20" }))], 1);
        let res = grade_one(&p, &r);
        assert!(!res.passed);
        assert_eq!(res.failures.len(), 1);
        assert!(
            res.failures[0].contains("forbidden tool `roll_dice` was called"),
            "unexpected failure: {:?}",
            res.failures
        );
    }

    #[test]
    fn grade_tool_called_with_input_contains() {
        let p = Prompt {
            id: "x".into(),
            description: String::new(),
            prompt: String::new(),
            lens: None,
            system_extra: None,
            expectations: vec![Expectation::ToolCalledWith {
                name: "search".into(),
                input_contains: json!({ "lens": "catholic" }),
            }],
        };
        // Right name, wrong lens → fail.
        let bad = rec("", &[("search", json!({ "lens": "lewisian" }))], 1);
        assert!(!grade_one(&p, &bad).passed);
        // Right name, right lens → pass.
        let good = rec(
            "",
            &[(
                "search",
                json!({ "lens": "catholic", "query": "purgatory" }),
            )],
            1,
        );
        assert!(grade_one(&p, &good).passed);
    }

    #[test]
    fn grade_iterations_cap() {
        let p = Prompt {
            id: "x".into(),
            description: String::new(),
            prompt: String::new(),
            lens: None,
            system_extra: None,
            expectations: vec![Expectation::IterationsAtMost { max: 3 }],
        };
        assert!(grade_one(&p, &rec("", &[], 3)).passed);
        assert!(!grade_one(&p, &rec("", &[], 4)).passed);
    }

    #[test]
    fn grade_text_contains_excludes_are_case_insensitive() {
        let contains = Prompt {
            id: "x".into(),
            description: String::new(),
            prompt: String::new(),
            lens: None,
            system_extra: None,
            expectations: vec![Expectation::TextContains { needle: "Soothe".into() }],
        };
        assert!(grade_one(&contains, &rec("Yes, use SOOTHE.", &[], 1)).passed);

        let excludes = Prompt {
            id: "x".into(),
            description: String::new(),
            prompt: String::new(),
            lens: None,
            system_extra: None,
            expectations: vec![Expectation::TextExcludes {
                needle: "must recite".into(),
            }],
        };
        assert!(grade_one(&excludes, &rec("No, you don't need to recite.", &[], 1)).passed);
        assert!(!grade_one(&excludes, &rec("You MUST recite the verse.", &[], 1)).passed);
    }

    #[test]
    fn bundled_suite_loads_and_parses() {
        let suite = load_bundled_suite().expect("bundled suite parses");
        assert!(suite.len() >= 6);
        // Every prompt has at least one expectation.
        for p in &suite {
            assert!(!p.expectations.is_empty(), "{} has no expectations", p.id);
        }
    }
}
