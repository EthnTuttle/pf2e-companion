//! Tool registry + agent loop.
//!
//! Phase 6 § D wires the existing PF2e validators (Phase 1) and the
//! hybrid search (Phase 6 § C) as agent-callable tools, then drives the
//! tool-use loop:
//!
//! 1. Caller hands us `messages + tools + opts`.
//! 2. We call `provider.chat()` and forward `Text` chunks to the UI.
//! 3. If the model emits `ToolCall` chunks, we execute them locally,
//!    append `Tool` messages to the conversation, and recurse.
//! 4. Hard cap at 5 iterations to bound latency / runaway loops.
//!
//! The wired tools, in alphabetical order:
//!   - `lookup_alias(name)` — legacy → Remaster name table
//!   - `lookup_miracle(query)` — biblical-miracle → spell map
//!   - `roll_dice(expr, secret?)` — live-table dice (Phase 10;
//!     session-scoped via `dispatch_tool_session`, GM/host-only)
//!   - `search(query, lens?)` — hybrid FTS+vector lens content
//!   - `validate_statblock(statblock)` — sanctification + license checks
//!   - `xp_budget(party_size, difficulty)` — encounter budget
//!
//! Each tool's local handler is a thin async wrapper around the existing
//! sync logic in `rules.rs` / `vault_write.rs` / `rag.rs`. The registry
//! is built once per agent invocation; cost is negligible.

use crate::db::Db;
use crate::llm::{
    ChatChunk, ChatOpts, LlmRegistry, Message, Role, ToolCall, ToolSpec, UsageSummary,
};
use crate::rag;
use crate::rules::{self, Difficulty};
use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use rusqlite::params;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Tool ids — kept as a typed list so the dispatch + spec advertise from
/// a single source of truth. Phase 10 adds the session-scoped
/// `roll_dice` (GM-only, host-only — see `dispatch_tool_session`).
pub const TOOL_IDS: &[&str] = &[
    "lookup_alias",
    "lookup_miracle",
    "roll_dice",
    "search",
    "validate_statblock",
    "xp_budget",
];

/// Build the tool spec list for the agent loop. The schemas mirror the
/// IPC commands' parameter shapes so a model that emits a valid
/// `tool_use` block produces a directly-usable arg map.
pub fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "lookup_alias".to_string(),
            description: "Look up a PF2e legacy → Remaster name (e.g. \"Heal\" → \"Soothe\"). \
                          Use for any monster, spell, or feat the user names that might use the \
                          older terminology."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Name to translate (case-insensitive)." }
                },
                "required": ["name"]
            }),
        },
        ToolSpec {
            name: "lookup_miracle".to_string(),
            description: "Map a biblical miracle (e.g. \"fed the 5000\", \"parted the Red Sea\") \
                          to its PF2e Remaster spell + sanctification. Returns Bible reference, \
                          tradition, and notes."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Miracle name, Bible reference, or spell name." }
                },
                "required": ["query"]
            }),
        },
        ToolSpec {
            name: "roll_dice".to_string(),
            description: "Roll PF2e dice notation (e.g. \"2d6+3\", \"d20\", \"4d6k3\") at the \
                          live table. Only available while you are hosting a table session as \
                          the GM; the result is posted to the shared chat/roll log."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "expr": { "type": "string", "description": "Dice expression, e.g. \"2d6+3\"." },
                    "secret": { "type": "boolean", "description": "GM-only hidden roll (default false)." }
                },
                "required": ["expr"]
            }),
        },
        ToolSpec {
            name: "search".to_string(),
            description: "Hybrid FTS+vector search over the bundled lens content. Use when the \
                          user asks about a doctrine, deity, saint, miracle, or world fact that \
                          might live in the active lens pack."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What to search for. Paraphrase the user's question." },
                    "lens":  { "type": "string", "description": "Optional lens id (lewisian/catholic/reformed/pentecostal/orthodox)." }
                },
                "required": ["query"]
            }),
        },
        ToolSpec {
            name: "validate_statblock".to_string(),
            description: "Validate a PF2e Remaster statblock JSON: checks sanctification \
                          (holy/unholy/spirit/vitality/void), license-provenance, and required \
                          fields. Returns errors + warnings."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "statblock": { "type": "object", "description": "The statblock JSON to validate." }
                },
                "required": ["statblock"]
            }),
        },
        ToolSpec {
            name: "xp_budget".to_string(),
            description: "PF2e Remaster encounter XP budget. Use to size encounters: returns \
                          base budget + per-PC adjustment for the given party size & difficulty."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "party_size": { "type": "integer", "minimum": 1, "maximum": 12 },
                    "difficulty": {
                        "type": "string",
                        "enum": ["trivial", "low", "moderate", "severe", "extreme"]
                    }
                },
                "required": ["party_size", "difficulty"]
            }),
        },
    ]
}

/// Execute one tool call, returning the JSON-encoded result string. Any
/// error is returned as an `Err` whose message will be fed back to the
/// model as the tool result (so it can recover or report).
///
/// `roll_dice` is intentionally NOT handled here — it requires an
/// active table session and is only reachable through
/// [`dispatch_tool_session`].
pub async fn dispatch_tool(
    call: &ToolCall,
    db: &Db,
    registry: &LlmRegistry,
) -> Result<Value> {
    match call.name.as_str() {
        "lookup_alias" => {
            let name: String = call.input.get("name")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .ok_or_else(|| anyhow!("lookup_alias.name missing"))?;
            tool_lookup_alias(db, &name)
        }
        "lookup_miracle" => {
            let q: String = call.input.get("query")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .ok_or_else(|| anyhow!("lookup_miracle.query missing"))?;
            tool_lookup_miracle(db, &q)
        }
        "search" => {
            let query = call.input.get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("search.query missing"))?
                .to_string();
            let lens = call.input.get("lens").and_then(|v| v.as_str())
                .unwrap_or("lewisian").to_string();
            let hits = rag::hybrid_search(db, registry, &query, &lens).await?;
            // Truncate to top-10 to keep the tool result reasonably sized.
            let trimmed: Vec<_> = hits.into_iter().take(10).collect();
            Ok(serde_json::to_value(trimmed)?)
        }
        "validate_statblock" => {
            let sb = call.input.get("statblock")
                .ok_or_else(|| anyhow!("validate_statblock.statblock missing"))?;
            let res = rules::validate_statblock(sb);
            Ok(serde_json::to_value(res)?)
        }
        "xp_budget" => {
            let party_size = call.input.get("party_size")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| anyhow!("xp_budget.party_size missing"))?
                as u8;
            let diff_s = call.input.get("difficulty")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("xp_budget.difficulty missing"))?;
            let diff: Difficulty = serde_json::from_value(json!(diff_s))?;
            if party_size == 0 || party_size > 12 {
                return Err(anyhow!("party_size {party_size} outside 1..=12"));
            }
            Ok(json!({
                "party_size": party_size,
                "difficulty": diff,
                "xp_budget": rules::xp_budget(party_size, diff),
                "per_pc_adjust": diff.per_pc_adjust(),
                "base_for_party_of_4": diff.base_budget_for_party_of_4(),
            }))
        }
        other => Err(anyhow!("unknown tool `{other}`")),
    }
}

fn tool_lookup_alias(db: &Db, name: &str) -> Result<Value> {
    let conn = db.conn.lock().unwrap();
    let needle = format!("%{name}%");
    let mut stmt = conn.prepare(
        "SELECT legacy_name, remaster_name, category, notes FROM remaster_aliases
         WHERE legacy_name LIKE ?1 COLLATE NOCASE
            OR remaster_name LIKE ?1 COLLATE NOCASE
         LIMIT 20",
    )?;
    let rows: Vec<Value> = stmt
        .query_map(params![needle], |r| {
            Ok(json!({
                "legacy_name": r.get::<_, String>(0)?,
                "remaster_name": r.get::<_, String>(1)?,
                "category": r.get::<_, String>(2)?,
                "notes": r.get::<_, Option<String>>(3)?,
            }))
        })?
        .collect::<Result<_, _>>()?;
    Ok(json!({ "matches": rows }))
}

fn tool_lookup_miracle(db: &Db, query: &str) -> Result<Value> {
    let conn = db.conn.lock().unwrap();
    let needle = format!("%{query}%");
    let mut stmt = conn.prepare(
        "SELECT miracle, reference, book, spell_name, tradition, sanctification, notes
         FROM miracle_spell_map
         WHERE miracle LIKE ?1 COLLATE NOCASE
            OR reference LIKE ?1 COLLATE NOCASE
            OR spell_name LIKE ?1 COLLATE NOCASE
            OR book = ?2 COLLATE NOCASE
         LIMIT 20",
    )?;
    let rows: Vec<Value> = stmt
        .query_map(params![needle, query], |r| {
            Ok(json!({
                "miracle": r.get::<_, String>(0)?,
                "reference": r.get::<_, String>(1)?,
                "book": r.get::<_, String>(2)?,
                "spell_name": r.get::<_, String>(3)?,
                "tradition": r.get::<_, Option<String>>(4)?,
                "sanctification": r.get::<_, Option<String>>(5)?,
                "notes": r.get::<_, Option<String>>(6)?,
            }))
        })?
        .collect::<Result<_, _>>()?;
    Ok(json!({ "matches": rows }))
}

/// Maximum tool-calling iterations per user turn. Matches the wiki
/// recommendation; tune later if sessions show recovery patterns that
/// need more depth.
pub const MAX_ITERATIONS: usize = 5;

/// Stream events emitted to the UI during an agent run. These are the
/// structured-content equivalent of token events; the IPC layer flattens
/// them into Tauri events with a simple discriminator.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// A piece of assistant prose. Concatenate to display.
    Text(String),
    /// The model has issued a tool call. UI may show a "calling X" hint.
    ToolStart { id: String, name: String, input: Value },
    /// A tool result was appended to the conversation. UI may show the
    /// JSON result in a collapsible block.
    ToolResult { id: String, name: String, result: Value, error: bool },
    /// Final usage summary at the end of the run.
    End { usage: UsageSummary, iterations: usize },
}

/// Session-aware tool dispatch (Phase 10). `roll_dice` requires an
/// active session handle and applies `Intent::RollDice` as the GM peer
/// of the hosted session (GM-only, host-only — rejected on processes
/// without a session handle). Everything else delegates to
/// [`dispatch_tool`].
pub async fn dispatch_tool_session(
    call: &ToolCall,
    db: &Db,
    registry: &LlmRegistry,
    session: Option<&std::sync::Arc<crate::session::SessionHandle>>,
) -> Result<Value> {
    if call.name == "roll_dice" {
        let handle = session
            .ok_or_else(|| anyhow!("roll_dice requires an active hosted table session"))?;
        if handle.role != crate::session::Role::Gm {
            anyhow::bail!("roll_dice is only available to the GM on the session host");
        }
        let expr: String = call
            .input
            .get("expr")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| anyhow!("roll_dice.expr missing"))?;
        let secret = call
            .input
            .get("secret")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        // Secret rolls store no numbers in the shared doc — the numeric
        // result is acked here (host session only) via the tracked roll.
        let (events, roll) = handle
            .intent_events(crate::session::Intent::RollDice { expr, secret })
            .await?;
        if let Some(roll) = roll {
            return Ok(json!({
                "expr": roll.expr,
                "total": roll.total,
                "rolls": roll.rolls,
            }));
        }
        // Fall back to the derived chat events (defensive; the tracked
        // roll is always present for RollDice intents).
        for event in events {
            if let crate::session::PatchEventPayload::ChatAppended { entry } = event.payload {
                if let Some(roll) = entry.roll {
                    return Ok(json!({
                        "expr": roll.expr,
                        "total": roll.total,
                        "rolls": roll.rolls,
                    }));
                }
            }
        }
        anyhow::bail!("roll_dice produced no roll record");
    } else {
        dispatch_tool(call, db, registry).await
    }
}

/// Drive the agent loop. The caller is expected to spawn this on a tokio
/// task and consume the receiver to forward events to the UI.
///
/// `system` is the seed system prompt. `messages` is the conversation so
/// far (no system message — `system` is hoisted by providers). The
/// returned Receiver yields one `AgentEvent` per emit until the run
/// completes or the iteration cap is hit.
pub fn run_agent(
    db: Arc<Db>,
    registry: Arc<LlmRegistry>,
    messages: Vec<Message>,
    system: String,
    cache_system: bool,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
) -> mpsc::UnboundedReceiver<Result<AgentEvent>> {
    run_agent_with_session(
        db,
        registry,
        messages,
        system,
        cache_system,
        temperature,
        max_tokens,
        None,
    )
}

/// [`run_agent`] with an optional live-table session handle. When a
/// GM-hosted session is active, the `roll_dice` tool rolls into the
/// shared session log; behavior is otherwise identical.
#[allow(clippy::too_many_arguments)] // mirrors the frozen `run_agent` shape + session
pub fn run_agent_with_session(
    db: Arc<Db>,
    registry: Arc<LlmRegistry>,
    mut messages: Vec<Message>,
    system: String,
    cache_system: bool,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    session: Option<Arc<crate::session::SessionHandle>>,
) -> mpsc::UnboundedReceiver<Result<AgentEvent>> {
    let (tx, rx) = mpsc::unbounded_channel::<Result<AgentEvent>>();
    let tools = tool_specs();

    tokio::spawn(async move {
        let mut total_usage = UsageSummary::default();
        let mut iterations = 0usize;

        loop {
            iterations += 1;
            if iterations > MAX_ITERATIONS {
                let _ = tx.send(Err(anyhow!(
                    "agent hit the {MAX_ITERATIONS}-iteration cap; aborting"
                )));
                break;
            }

            let opts = ChatOpts {
                max_tokens,
                temperature,
                system: Some(system.clone()),
                tools: tools.clone(),
                cache_system,
            };

            // Acquire a stream under the read guard, then drop it before
            // consuming (the stream future doesn't need the registry).
            let stream_result = {
                let guard = registry.read().await;
                match guard.as_ref() {
                    Some(active) => active.provider().chat(messages.clone(), opts).await,
                    None => Err(anyhow!("llm provider not configured")),
                }
            };

            let mut stream = match stream_result {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(Err(e));
                    return;
                }
            };

            let mut assistant_text = String::new();
            let mut pending_tool_calls: Vec<ToolCall> = Vec::new();

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(ChatChunk::Text(s)) => {
                        assistant_text.push_str(&s);
                        if tx.send(Ok(AgentEvent::Text(s))).is_err() {
                            return; // receiver dropped
                        }
                    }
                    Ok(ChatChunk::ToolCall(tc)) => {
                        let _ = tx.send(Ok(AgentEvent::ToolStart {
                            id: tc.id.clone(),
                            name: tc.name.clone(),
                            input: tc.input.clone(),
                        }));
                        pending_tool_calls.push(tc);
                    }
                    Ok(ChatChunk::End { usage }) => {
                        if let Some(u) = usage {
                            total_usage.input_tokens = sum_opt(total_usage.input_tokens, u.input_tokens);
                            total_usage.output_tokens = sum_opt(total_usage.output_tokens, u.output_tokens);
                            total_usage.cache_read_input_tokens =
                                sum_opt(total_usage.cache_read_input_tokens, u.cache_read_input_tokens);
                            total_usage.cache_creation_input_tokens =
                                sum_opt(total_usage.cache_creation_input_tokens, u.cache_creation_input_tokens);
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                }
            }

            // No tool calls → assistant turn is final, exit the loop.
            if pending_tool_calls.is_empty() {
                let _ = tx.send(Ok(AgentEvent::End {
                    usage: total_usage,
                    iterations,
                }));
                return;
            }

            // Append the assistant turn (text + tool_calls).
            messages.push(Message {
                role: Role::Assistant,
                content: assistant_text,
                tool_calls: pending_tool_calls.clone(),
                tool_call_id: None,
            });

            // Execute every tool call and append a Tool message for each.
            for tc in &pending_tool_calls {
                let (result_value, is_err) =
                    match dispatch_tool_session(tc, &db, &registry, session.as_ref()).await {
                    Ok(v) => (v, false),
                    Err(e) => (json!({ "error": e.to_string() }), true),
                };
                let _ = tx.send(Ok(AgentEvent::ToolResult {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    result: result_value.clone(),
                    error: is_err,
                }));
                let serialized = match serde_json::to_string(&result_value) {
                    Ok(s) => s,
                    Err(e) => format!("{{\"error\":\"serialize tool result: {e}\"}}"),
                };
                messages.push(Message {
                    role: Role::Tool,
                    content: serialized,
                    tool_calls: Vec::new(),
                    tool_call_id: Some(tc.id.clone()),
                });
            }
            // Loop back to the model.
        }
    });

    rx
}

fn sum_opt(a: Option<u32>, b: Option<u32>) -> Option<u32> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a + b),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_specs_lists_all_five_tools() {
        let specs = tool_specs();
        let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
        for id in TOOL_IDS {
            assert!(names.contains(id), "tool_specs missing `{id}`");
        }
    }

    #[test]
    fn sum_opt_combines_both_sides() {
        assert_eq!(sum_opt(None, None), None);
        assert_eq!(sum_opt(Some(2), None), Some(2));
        assert_eq!(sum_opt(None, Some(3)), Some(3));
        assert_eq!(sum_opt(Some(2), Some(3)), Some(5));
    }
}
