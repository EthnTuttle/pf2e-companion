# Shipping Broadly — Roadmap Beyond Phase 11

Status at this doc: desktop + LAN-mobile + iroh WAN-relay sync all test-proven;
GUI boots (xvfb smoke); two playable originals adventures; 167/86 tests green.
This file is the prioritized path from "works on my machines" to "groups of
friends actually play this". Tier 1 = minimum for a friends beta. Tier 2 =
public release. Tier 3 = long-term health.

## Tier 1 — Friends beta (playable across real devices)

1. **Real-device smoke** (the only never-executed gate): run `pnpm tauri dev`
   on a physical desktop, `pnpm tauri android dev` on a real phone, one full
   table session (host → PIN join → roll → handout → end/resume). Time-box 1
   evening; file issues; fix blockers only.
   - Android: add `ACCESS_WIFI_STATE`/`CHANGE_WIFI_MULTICAST_STATE` +
     `INTERNET` to the merged manifest if LAN discovery is added later; bare
     WS needs nothing extra. Test Local Network permission copy on iOS 14+
     (the LSAL dialog) — first launch must explain *why*.
2. **Session invite deep-links**: register `pf2e://join?url=…&pin=…` scheme +
   universal-link/App-Intent so "text your friend a link" replaces typing
   host:port+PIN. (iroh tickets remain for desktop-desktop.)
3. **Release installers**: `pnpm tauri build` (bundled) for Linux AppImage/deb,
   Windows msi, macOS dmg — unsigned is fine for a friends beta; document
   Gatekeeper/SmartScreen bypass clicks. Keep `--no-bundle` CI matrix.
4. **Crash + log hygiene**: `tracing` already present — add a ring-buffer log
   file per session under the vault dir + "Copy diagnostics" button in
   Settings (paste-ready for GitHub issues). No telemetry yet.
5. **Save UX**: sessions auto-save already; add "Export session log"
   (markdown transcript) to the EndSession flow — doubles as session notes.
6. **First-run wizard**: 3 screens (vault location, lens pick, LLM optional /
   BYO-key) replacing today's hunt-through-Settings.

## Tier 2 — Public release

7. **Signing & stores** (extends the existing Phase 8 plan): Apple
   Developer + notarization, Windows code-signing cert, Play + App Store
   listings, TestFlight/Play-internal beta tracks. CI already cross-compiles;
   add signed artifacts per tag.
8. **Licensing pass (critical before public)**: app code stays MIT; stats use
   ORC; add the **Paizo Community Use Policy** notice + "compatible with
   Pathfinder Second Edition" legend in About + installers; confirm no
   Product Identity (Golarion terms) is referenced; audit bundled content for
   full `sources` arrays (Phase 5 pattern continues).
9. **LLM hardening for strangers' tables**: table-content-filter system layer
   (respectful Christian tone enforcement, no denominational attacks), eval
   suite additions for toxicity/hallucination probes, per-table token budget
   caps visible to the GM, Ollama default suggestion for zero-cost tables.
10. **P2P trust upgrade**: per-peer ed25519 identity → PIN upgrade from shared
    secret to capability tokens; kick/ban intent (GM); TLS-ish framing is
    already QUIC on iroh; WS-LAN stays PIN (document threat model).
11. **Mobile store UX**: bottom-tabs flow audit on 5" screen, haptics on your
    turn, keep-awake flag during sessions, battery profile for the WS.
12. **Foundry import/export** (existing Phase 7 stub): rounds-trip statblocks
    so existing PF2e communities can migrate content in/out.
13. **Docs site**: mdbook generated from `docs/` + in-app tutorial overlay on
    first `/table` open; short loom-style video: 90-second "host your first
    Widow's Lamp table".

## Tier 3 — Long-term health

14. **Automerge doc migrations**: `doc_version` in meta + migration chain
    (session saves from v1 must load forever); property tests with arbitrary
    doc shapes.
15. **Observability opt-in**: anonymous crash ingestion (self-hosted Sentry or
    none), perf markers around sync pump; zero tracking by default (it's a
    values-differentiator for this audience — say so in marketing).
16. **Pack ecosystem**: content-pack format v2 with semver + signatures;
    community-submitted adventures reviewed via PR + `validate_statblock` CI
    gate; in-app pack browser.
17. **Rules depth**: conditions auto-effects, attacks/DC automation with the
    degree-of-success ladder, map layers/FoW if the table-demands data says so
    (don't build ahead of playtests).
18. **Federation/rendezvous**: optional self-hostable rendezvous for friend
    groups (cheap DO droplet), still serverless-by-default.

## Explicitly out of scope (until playtests say otherwise)

- Voice/video (use Discord), 3D, marketplace payments, account system/cloud
  saves (the vault is the account), denominational content arbitration beyond
  the existing 5-pack lens system (expand only via the wiki research flow).

## Standing guardrails (never regress)

- All authority checks server-side; players can only touch their own domains.
- Secret rolls never enter the shared doc.
- SQLite stays derived; markdown/automerge files stay canonical.
- `cargo test` / `pnpm test` / `pnpm check` / xvfb smoke green before push;
  `PF2E_NET_WAN=1` manually before any release tag.
