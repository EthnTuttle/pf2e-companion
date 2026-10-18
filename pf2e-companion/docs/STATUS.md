# Handoff status — 2026-10-15 (friends-beta integration + ship cut)

Branch **`master` is canonical** (`main` is frozen at `fbdd268`, fully contained
in master via the reconciliation merge — kept for history, do not branch from it).
This file is the running "where we are" for whichever machine picks up next;
`docs/SHIPPING.md` remains the roadmap. **Today 08:00 EDT both machines should
converge on master.**

## Where this came from

Two sessions diverged at `c5ae8a0` (SHIPPING.md): this machine built the Phase 12
E2E harness + imagery v1; the other machine built Tier-1 phases 12–15 (session
log export, diagnostics hub, first-run wizard, UI-shots harness). Orchestrated
merge landed everything:

- `81a5a20` fork reconciliation merge (4 conflicts resolved by hand; lockfiles
  regenerated; `e2e/node-shims.d.ts` widened to reconcile both harnesses)
- S24 upgraded to a **real resume-saved-session** scenario (lobby's saved
  sessions panel was the old xfail pin's replacement case)
- `b27af5e` installers — local `pnpm tauri build` proven (deb 18.4 MiB /
  AppImage 92.8 MiB / rpm) + `v*`-tag `release.yml` (win msi / mac dmg /
  linux deb+appimage → GitHub Release assets). Fixed a red `tauri build`:
  npm `@tauri-apps/plugin-dialog` 2.8.2 vs cargo 2.7.3 skew — aligned to
  cargo 2.8.3 / npm api ^2.12.3 (specifier is the drift guard)
- `426a1ed` F1 tabbar fix — play-surface `100vh` ignored chrome; now `100%`
  inside a `100dvh` route-scoped shell; S27 stop 3 asserts composer clears
  the tabbar; shots matrix (80 PNGs) regenerated clean
- `0745e2f` deep-links — `pf2e://join?url=…(&pin=)` scheme, lobby prefill +
  history scrub, Copy-invite-link in the invite panel, 19 parse/build unit
  tests; rust-ini pinned 0.21.1 (0.21.3 breaks tauri-plugin-deep-link 2.6.2's
  compile — E0063)

## Final certification battery on the merged tree (solo, no sibling worktrees)

- `cargo test --all-features` — 24 suites, 0 failures
- `corepack pnpm check` — 0 errors / 0 warnings
- `corepack pnpm test` — 131/131 (112 + 19 deeplink)
- `corepack pnpm build` — ✓
- `corepack pnpm e2e` — 27/27 (deterministic lane)
- `PF2E_E2E_LAN=1 corepack pnpm e2e:live` — 4/4

## Release cut

Tag `v0.1.0` pushed to the fork fires `.github/workflows/release.yml` →
Release with deb/AppImage/msi/dmg assets. Unsigned: friends need the
first-launch bypass clicks documented in README → Install.

## Next up: fix the real-device findings (Tier 1 item 1 executed 2026-10-18)

Tier 1 item 1 (real-device evening) is DONE as a test-and-document pass — full
verdicts, exact error strings, and evidence filenames in
`docs/ui-reviews/2026-10-18-real-device.md`. Headline: the device story is
90% real — debug APK builds/installs first-try, `pf2e://` tap-through prefills
hot and cold, first-run wizard renders, and a real desktop host (Xvfb
WebKitGTK) accepted a real phone over true Wi-Fi. But three things must be
fixed before friends beta, in this order:

1. ~~F-R1 (blocker)~~ **FIXED on `fix-r1-join-lan`** — lobby LAN join no
   longer IPCs: the button hands off to `/table/play?host=…` and every shell
   (browser, native webview, deep-link prefill) joins via `wsTransport`.
   Live lane gained L5 (real button click incl. the prefilled variant);
   fantasy mock removed. See `docs/plans/PLAN-F-R1-join-lan.md`.
2. **F-R2 (blocker on VPN'd hosts)** — `local_lan_ip()`'s 8.8.8.8 UDP probe
   followed the Mullvad tunnel and advertised `ws://10.146.185.179:4780/…`
   (tunnel /32) instead of `192.168.1.172`; phone needed a hand-corrected URL.
   Enumerate non-tunnel interfaces / show all candidates / allow override.
3. **F-R3 (verify before shipping desktop bundle)** — GM "Enter Table →"
   anchor does not navigate on the WebKitGTK dev build (pointer AND keyboard,
   focus-ring-verified; same-page buttons work; route proven from the phone).
   Reproduce with `WEBKIT_INSPECTOR_SERVER=127.0.0.1:9223`.

Secondary: F-R6 lobby "Join Session" button partially under the fixed tabbar
on native at 390px (F1 fix itself verified GOOD on the play surface), F-R7
phone→GM chat seam (known, matches `live-lan.spec.ts` header note).

## Sandbox quirks ledger

- **VPN hosts (F-R2)**: the old 8.8.8.8 UDP-probe LAN-IP trick follows
  policy routing — under Mullvad it returns the tunnel /32. Fixed by
  interface enumeration + ranking in `src-tauri/src/lan.rs`; note that
  `if-addrs`' POSIX `is_oper_up()` is strictly `IFF_RUNNING`, and
  WireGuard/tun interfaces report **UP**, so up-state can never detect a
  tunnel — the name/`/32`/CGNAT/192.168-first rules carry it.
- **Port 4173 contention**: sibling worktrees each spawn the Vite preview on
  4173; a second `pnpm e2e` run *attaches to the foreign server* (foreign code,
  missing harness → false failures like "Cannot read properties of undefined
  (invocations)"). Before trusting any e2e/live result: `ss -ltn | grep 4173`
  must be empty. Failure of this kind during today's verification was
  environmental, not code.
- **Cold worktree Rust**: a fresh worktree's first `e2e:live` dies in
  `beforeEach` compiling `e2e_host` — prime with `cargo run --example
  e2e_host` once, then the lane is green.
- Provider flaps killed two subagent sessions mid-flight ("Team doesn't
  exist", terminal-yield). Both times the work was recoverable: one resumed
  via a retry dispatch, one had already committed green (verify the commit's
  claimed gates before trusting, then proceed).
- `APPIMAGE_EXTRACT_AND_RUN=1` needed for `tauri build` in this machine's env
  (full env note in `docs/plans/PLAN-TIER1-installers.md`).
