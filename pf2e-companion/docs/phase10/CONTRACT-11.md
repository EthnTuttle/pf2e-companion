# Phase 11 — Ship-completeness & Content (FROZEN)

Extends `docs/phase10/CONTRACT.md` + `CONTRACT-10_5.md` (both stay frozen). Six items,
exactly as scoped. No new cargo/npm deps anywhere. Owners: RUST = `src-tauri/**`;
WEB = `src/**`, `src-tauri/data/content/**`, `README.md`.

## 1. GUI smoke test (RUST; env-gated, off by default)

New `src-tauri/tests/gui_smoke.sh` (executable, bash, `set -euo pipefail`):
```bash
# Self-gates: unless PF2E_GUI_SMOKE=1, print "SKIP gui smoke" and exit 0.
# Requires: xvfb-run, curl, pnpm. Budget: 45s hard timeout (timeout -s TERM 45).
# 1) pnpm build
# 2) xvfb-run -a pnpm tauri dev >"$TMP/gui-smoke.log" 2>&1 &  PID=$!
# 3) poll loop (1s × 40): success when ALL hold —
#    a) kill -0 $PID alive,
#    b) ! grep -qE 'panic|thread .*panicked|RUST_BACKTRACE' log,
#    c) curl -sf http://localhost:1420 → HTTP 200
# 4) EXIT: kill -TERM $PID (+ pkill -P /^vite$|^cargo$/ children), wait, exit status
```
No input automation. New `src-tauri/tests/phase11_gui_smoke.rs`:
`gui_smoke_launches_cleanly_under_xvfb` — env-guard on `PF2E_GUI_SMOKE=1` (early
`return` otherwise), `Command::new("bash").arg(concat!(env!("CARGO_MANIFEST_DIR"),
"/tests/gui_smoke.sh"))`, asserts exit success. README documents it.

## 2. WAN/relay iroh test (RUST; manual, env-gated)

`src-tauri/tests/phase10_p2p.rs` += ONE test `relay_peers_converge_via_n0_relay`:
same shape as `loopback_peers_converge_and_resync_on_change` but endpoints use
`Endpoint::builder(presets::N0)` with **`RelayMode::Default`** (no
`clear_address_lookup`), gated `PF2E_NET_WAN=1`, and the WHOLE converge flow wrapped
in `tokio::time::timeout(Duration::from_secs(10), …)` — timeout/connect failure
panics with: `"PF2E_NET_WAN: relayed connect failed — network sandbox may block
UDP/443 QUIC to n0 relays"`. Never hangs, never runs in default `cargo test`.

## 3. Eval additions (RUST owns eval.rs + tests; prompts.json shared/data)

- `eval.rs`: add ONE variant (per the file's own "add a variant + match arm" rule):
```rust
/// No ToolCall event named `name` may occur in the run.
ToolNotCalled { name: String },
```
with grader arm (fail if `rec.tool_calls.iter().any(|t| t.name == *name)`), unit
tests in eval.rs, and integration coverage in `phase6_5.rs`.
- `data/eval/prompts.json`: APPEND three prompts (suite grows 6 → 9):
  - `roll-dice-gm-table`: "We're at the table, I'm the GM — roll 2d6+3 for the
   NPC's damage." → `tool_called` roll_dice + `tool_called_with input_contains
    {"expr":"2d6+3"}`.
  - `roll-dice-secret`: GM asks for a hidden/perception roll → `tool_called_with`
    `{"secret":true}` accepted loose (`input_contains` superset).
  - `roll-dice-player-refusal`: "As a player, roll my attack for me" →
    `tool_not_called roll_dice` AND `text_contains` "~players roll their own~" or
    similar discipline phrase (build prompts so a compliant model must pass BOTH).

## 4. Second adventure (WEB authors; RUST generalizes the loader test)

WEB ships `src-tauri/data/content/lewisian/adventures/the-roof-of-mercy/`:
`roof-of-mercy.md` + `roof-of-mercy.<npc>.statblock.json` ×3 — same format as
`the-widows-lamp/` (CONTRACT §5 as implemented). Frozen content constraints:
- frontmatter `id: lewisian:adventure-roof-of-mercy`, `type: adventure`,
  `lens: lewisian`, `license_provenance: homebrew`, `level_range: [3, 4]`,
  exactly 3 `scenes` (exactly one with `encounter`, roster refs valid), exactly 1
  `handout`, `npcs: [3 slugs]` each with a sidecar that passes
  `rules::validate_statblock` (name/level/type + homebrew provenance).
- Theme: Mark 2:1–12 (roof opened to lower the paralytic; forgiveness given AND
  practiced between people). Respectful, ecumenical; no pastoral polemics; full
  `sources:` arrays on md + each sidecar (same convention as widows-lamp).
RUST: `phase10_adventure.rs` — keep ALL widows-lamp-specific asserts; ADD
count-agnostic `bundled_adventures_all_parse_and_validate` that enumerates every
`adventures/<dir>/*.md` under the embedded CONTENT_DIR, parses + validates each,
asserts `>= 2` adventures and empty problem lists. Do NOT relax existing asserts.

## 5. EntityEditor a11y cleanup (WEB)

`src/lib/EntityEditor.svelte`: drive its 11 svelte-check warnings to **0**
(labels↔inputs via `for`/`id`, aria on icon-buttons, keyboard handlers on clickable
non-button divs). Zero behavior change; `pnpm check` total warnings hits **0 repo-wide**.

## 6. README "Run your first game" (WEB, ≤40 lines, README voice)

New section after Architecture: host → showroom lobby PIN + `lan_url`/iroh ticket →
player join (desktop ticket vs mobile `?host=…&pin=…`) → LoadAdventure(widows-lamp)
→ first roll → secret GM roll behavior → handout reveal → EndSession → resume via
`session_load`. Also document `PF2E_GUI_SMOKE=1` / `PF2E_NET_WAN=1` under Develop.

## 7. Exact new/changed test names (grep-verifiable)

| File | Adds |
|---|---|
| `tests/phase11_gui_smoke.rs` | `gui_smoke_launches_cleanly_under_xvfb` |
| `tests/gui_smoke.sh` | (script, self-gating) |
| `tests/phase10_p2p.rs` | `relay_peers_converge_via_n0_relay` |
| `tests/phase10_adventure.rs` | `bundled_adventures_all_parse_and_validate` |
| `tests/phase6_5.rs` | `grader_rejects_when_forbidden_tool_called` + happy-path arm for the 3 new prompts |
| `src-tauri/src/eval.rs` (unit) | `tool_not_called_passes_when_absent`, `tool_not_called_fails_when_present` |
| `data/eval/prompts.json` | ids §3 (count 6→9) |

## 8. Guardrails

- Gates: `cargo test` + `--all-features` 0-fail (default path runs NEITHER gui nor
  WAN test — both early-return without their env vars); `pnpm test`,
  `pnpm check` = **0 errors / 0 warnings**; `pnpm build`.
- CI stays deterministic: `PF2E_GUI_SMOKE` / `PF2E_NET_WAN` default-unset; the two
  gated tests must never touch the network/display unless enabled.
- No new deps. GUI smoke uses xvfb-run+timeout+curl (system tools, not cargo/npm deps).
- Eval prompt ids are additive-append only; do not edit the existing six
  (`expectation` shapes frozen by `eval.rs` serde).
- Previous contracts untouched; anything ambiguous here is decided by amending THIS
  file, not by cross-agent edits.
