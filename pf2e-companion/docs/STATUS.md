# Handoff status — 2026-10-18 (Tier 1 CLOSED; device-evening fixes certified)

Branch **`master` is canonical** (`main` is frozen at `fbdd268`, fully contained
in master via the reconciliation merge — kept for history, do not branch from it).
This file is the running "where we are" for whichever machine picks up next;
`docs/SHIPPING.md` remains the roadmap.

## State of the ship

**Tier 1 is complete and certified.** All five slices landed, the real-device
evening ran (Pixel 7a, Android 16), every device-found blocker is fixed, and
the union tree passed the second full battery solo. `v0.1.0` is the ship tag.

Second certification battery (2026-10-18, solo — port verified free first):

- `cargo test --all-features` — 24 suites, 0 failures
- `corepack pnpm check` — 0 errors / 0 warnings
- `corepack pnpm test` — 134/134
- `corepack pnpm build` — ✓
- `corepack pnpm e2e` — 32/32 (27 original + 2 F-R1 lobby pins + 3 F-R7 ws pins)
- `PF2E_E2E_LAN=1 corepack pnpm e2e:live` — 6/6 (L5 join-via-real-button,
  L6 peer-outbound + role rejection)

## Where this came from

Two sessions diverged at `c5ae8a0` (SHIPPING.md): this machine built the Phase 12
E2E harness + imagery v1; the other machine built Tier-1 phases 12–15 (session
log export, diagnostics hub, first-run wizard, UI-shots harness). Orchestrated
merge landed everything, then the device evening + fix wave:

- `81a5a20` fork reconciliation merge (4 conflicts resolved by hand; lockfiles
  regenerated; `e2e/node-shims.d.ts` widened to reconcile both harnesses)
- S24 upgraded to a **real resume-saved-session** scenario
- `b27af5e` installers — local bundles proven + `v*`-tag `release.yml`; also
  fixed the npm/cargo plugin-dialog skew that had every `tauri build` red
- `426a1ed` F1 tabbar fix (play surface)
- `0745e2f` deep-links — `pf2e://join?…`, lobby prefill, Copy-invite-link
- `684bc3c` lockfile-drift repair (see ledger lesson below)
- `9f0d5fc` device-evening findings doc (`docs/ui-reviews/2026-10-18-real-device.md`)
- `62f97da` F-R2 — LAN advertise picks the real interface, not the VPN /32
  (`src-tauri/src/lan.rs` enumeration + pure ranked picker, `PLAN-F-R2`)
- `5e50d26` F-R1 — join unified on wsTransport, dead `session_join_lan` gone,
  `createTransport` honors `?host=` before Tauri internals, L5 pins it
  (`PLAN-F-R1-join-lan.md`)
- `5798fea` F-R7 — play page's 15 intent call sites ride their transport;
  peer→GM chat lives; role-authority verified live (L6); **zero wire change**
  (`PLAN-F-R7-peer-outbound.md`)
- F-R3/F-R6 — `/table/play` mount crash was a dev-only Svelte
  `bind_invalid_checkbox_value` (fixed `bind:value`→`bind:checked` on the
  Secret toggle); 390px native lobby CTA pinned above the tabbar with
  measured proof; WebKitGTK inspector recipe captured for future native
  debugging (`docs/ui-reviews/2026-10-18-native-followups.md`)

## Release cut

Tag `v0.1.0` → `.github/workflows/release.yml` → GitHub Release with
deb/AppImage/msi/dmg. First tag (2026-10-16) died at frozen-install because
of the lockfile drift; tag deleted, moved to the certified tip, re-pushed.
Unsigned: friends need the first-launch bypass clicks in README → Install.

## Next up (post-beta)

- Tier 2 per `docs/SHIPPING.md` (licensing pass incl. AI-art disclosure page
  note, etc.)
- Remaining first-pass ui-review findings beyond F1 (`2026-10-14-first-pass.md`)
- Watch field reports; the LAN-advertise ranking is heuristic on interface
  names (documented in `lan.rs` + ledger)

## Sandbox quirks / honest-lessons ledger

- **Merge commits can encode stale locks**: staging `--ours` lockfiles and
  THEN regenerating them leaves the working tree — not the commit — correct.
  Symptom arrived two days later via a red frozen-install release CI. Rule:
  regenerate FIRST, stage LAST; `git status` the lockfiles before committing
  any merge touching manifests. (Fixed in `684bc3c`; v0.1.0 retagged.)
- **Dev-only Svelte errors are invisible to the prod e2e harness**: F-R3's
  mount crash existed only under `tauri dev`/dev-mode; screenshots + prod
  spec lanes were uniformly green. When a native-shell "X is dead" report
  contradicts the harness, suspect a dev-only throw first, and instrument via
  `WEBKIT_INSPECTOR_SERVER` (GVariant framing + Target.sendMessageToTarget;
  recipe in `2026-10-18-native-followups.md`).
- **Fantasy IPC stubs erase bugs**: the deterministic lane pinned a
  `session_join_lan` response for a command that never existed — green
  harness, dead button on hardware. Any new harness stub must name a
  registered handler; audit periodically (`F-R1 plan` has the audit recipe).
- **VPN hosts (F-R2)**: interface-name heuristics + `/32`+CGNAT exclusion now
  pick the Wi-Fi iface; `if-addrs`' POSIX `is_oper_up()` is strictly
  `IFF_RUNNING` and WireGuard reports UP — up-state never detects a tunnel.
- **Port 4173 contention**: sibling worktrees each spawn the Vite preview on
  4173; a second run *attaches to the foreign server* (foreign code, missing
  harness → "Cannot read properties of undefined (invocations)").
  `ss -ltn | grep 4173` must be empty before trusting any e2e/live result.
- **Cold worktree Rust**: first `e2e:live` in a fresh worktree dies in
  `beforeEach` compiling `e2e_host` — prime with `cargo run --example
  e2e_host` once.
- Provider flaps killed three subagent sessions mid-flight ("Team doesn't
  exist", terminal-yield). Every time the work survived in a commit — verify
  the committed gates, then proceed; don't re-dispatch blindly.
- `APPIMAGE_EXTRACT_AND_RUN=1` needed for `tauri build` in this machine's env
  (full env note in `docs/plans/PLAN-TIER1-installers.md`).
- **Local disk bloat compounds fast**: the 2026-10-20 hygiene pass reclaimed
  **17 GB** (repo tree 83 GB → 66 GB), nearly all of it cargo output —
  `target/release` (2.1 GB), `target/debug/examples` (2.2 GB), and four
  stale Android-ABI triple dirs under `src-tauri/target/` (13.2 GB) left
  over from the Oct-18 device-lab certification. Reclaim with
  `pnpm clean:artifacts` (see below); re-audit with `du -sh` when the disk
  feels heavy.

## Local artifact footprint

Disk weight in this repo is dominated by `src-tauri/target/` (cargo). After
the 2026-10-20 pass it is ~59 GB, of which `target/debug` deps/incremental
is stale-prunable in theory but kept warm to avoid a cold rebuild.

What's regenerable (all gitignored, safe to delete):

- `src-tauri/target/release/` — `pnpm tauri build` output; local devs should
  stay on the debug profile, release targeting is CI's job.
- `src-tauri/target/<android-triple>/` — per-ABI debug outputs from
  `tauri android build`; delete when device work is done for a milestone
  (one-off: 13.2 GB reclaimed 2026-10-20 post-certification).
- `test-results/`, `playwright-report/`, `tests/e2e/screenshots/`, `shots/`
  — regenerated by every Playwright / shots-harness run.
- `tools/imagery/staging/`, `tools/imagery/review.html` — imagery-pipeline
  transients (`review.html` is rendered from staging output).
- `.svelte-kit/` — regenerated by `pnpm check`/`build`/`dev`.

Reclaim the routine set with:

```bash
pnpm clean:artifacts
```

Kept on purpose: `node_modules/` (pnpm-store-linked, needed for dev),
`device-lab/` (physical-device captures; not regenerable without the phone),
`tools/imagery/.venv/` (pip install cost), `src-tauri/gen/` (mobile project
sources — note its `android/app/build/` is pure Gradle output, 6.1 GB, and
can be reclaimed with a Gradle clean when Android work goes cold),
`target/debug` core (warm dev cycle).
