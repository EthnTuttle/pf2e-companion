# Handoff status — 2026-10-14 (friends-beta push, agent-orchestrated)

Branch `main`, all work committed. This file is the running "where we are" for
whichever machine picks up next; `docs/SHIPPING.md` remains the roadmap.

## Landed this session (7 commits)

- `cd01af4` docs: nostr analysis (`docs/research/nostr-integration-analysis.md` —
  verdict: **no nostr in core**; only Tier-3-conditional signed pack announcements;
  **no zaps in-app**, Apple 3.1.1 + out-of-scope list + audience fit) + LN-tip
  funding: `.github/FUNDING.yml` → `average@minibits.cash`, README Support section.
- `02acd14` phase 12: session log export — markdown transcript (GM drawer pre-end
  + lobby saved-sessions Resume/Export post-end), `tauri-plugin-dialog` save-as
  (platform-scoped: linux=xdg-portal, others upstream defaults — review catch),
  vault watcher skips `sessions/` artifacts.
- `7ebac15` phase 13: diagnostics — 2,000-line ring + per-session `.log` twin
  (host-only, append-across-resume), `diagnostics_get` IPC, **new `/settings`
  index page** with Copy-diagnostics (no telemetry). Peer=ring-only.
- `853a123` phase 14: first-run wizard — `/onboarding` 3 screens (vault / lens /
  compact LLM), configurable vault root via hand-rolled atomic `settings.json`
  (no new deps; restart on vault change; existing installs auto-skip),
  lens persists to localStorage.
- `3a8dfde` docs: UI-shots plan.
- `28844eb` phase 15: UI screenshot harness — `pnpm e2e:shots` boots the real app
  in headless Chromium (init-script `__TAURI_INTERNALS__` stub + typed fixtures;
  wizard stubs default to completed; chat-reply replay labeled stubbed-backend),
  writes 80 PNGs (20 routes × desktop/mobile × light/dark) to gitignored `shots/`
  + `manifest.json`. Not in CI/default gates.
- `51f1ebd` docs: first visual review (`docs/ui-reviews/2026-10-14-first-pass.md`).

Gates at handoff: 194 cargo (default + `--all-features`) / 112 vitest /
`pnpm check` 0-err (1 known pre-existing tsconfig @types/node warning) /
`pnpm build` / `pnpm e2e:shots` exit 0.

## NEXT: what to pick up

1. **F1 fix slice (recommended next)** — from the UI review: the fixed bottom
   tab bar overlaps scroll content and, on `/table/play`, the chat composer
   (mobile worst: composer half-hidden behind tabs). Then regenerate shots and
   confirm in a second review pass. Good first-cycle on a fresh machine.
2. **Real-device evening** (SHIPPING Tier 1 #1, human) — full session host →
   PIN-join → roll → handout → end/resume on real hardware. Pair with
   **deep-links** (item 2) since it needs the same device session, and run
   `PF2E_GUI_SMOKE=1 cargo test --test phase11_gui` once here if the machine
   has xvfb (this sandbox lacks `xvfb-run`; phase-14 startup order changed
   since it last ran).
3. Remaining Tier 1: installer config (#3, needs a cut build), first-run value
   proven once device evening happens.

## Agent pipeline conventions (so the next session reproduces it)

- Roles: **plan** `tff/glm-5.3#high` → **implement** `tff/glm-5.3#medium` (or
  `tff/nemotron-3-super#medium`) → **review** `tff/nemotron-3-ultra#high`.
- Orchestrator trust-but-verifies every implementer against `git status` + the
  four gates before spawning the reviewer. Reviewers caught real regressions
  (dialog backend strip) — keep them in the loop.
- Known-bad models on this host today: `deepseek-v4.1-flash` (DSML tool-call
  leak), `glm-5.3-flash` (silent zero-work / truncation). tff provider flapped
  twice (Hosted_vllmException 500s); background long commands survive restarts.
- Every slice gets a plan in `docs/plans/`; UI feedback lands in
  `docs/ui-reviews/<date>-<scope>.md` (`docs/ui-reviews/README.md` has the
  reviewer prompt template).

## Sandbox quirks (this machine)

- `pnpm` lives at `~/.nvm/versions/node/v24.12.0/bin` (nvm; not on default PATH).
- All cargo invocations need `PKG_CONFIG_PATH=/tmp/opencode/gtk-shim/pkgconfig`
  (no GTK dev packages; shim is .pc files over system runtime libs).
- Playwright chromium cached at `~/.cache/ms-playwright/`; fresh machines need
  `pnpm exec playwright install chromium` once.
- Env-gated suites stay OFF by default: `PF2E_NET_TESTS`, `PF2E_NET_WAN`,
  `PF2E_GUI_SMOKE`.
