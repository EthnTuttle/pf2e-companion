# PLAN — Tier 1 slice: "Release installers" (SHIPPING.md item 3)

Status: **IMPLEMENTED** on branch `ship-installers`. Source of truth:
`docs/SHIPPING.md` § Tier 1 item 3: "`pnpm tauri build` (bundled) for Linux
AppImage/deb, Windows msi, macOS dmg — unsigned is fine for friends beta;
document Gatekeeper/SmartScreen bypass clicks; keep the existing `--no-bundle`
CI matrix." No changes to prior contracts; `build.yml` untouched; version stays
0.1.0; icons and signing config untouched.

## 1. The blocker found first: lockfile desync kills ALL tauri builds

Before any bundling could be attempted, `pnpm tauri build` failed in <1s:

```
Error Found version mismatched Tauri packages. Make sure the NPM package and
Rust crate versions are on the same major/minor releases:
tauri-plugin-dialog (v2.7.3) : @tauri-apps/plugin-dialog (v2.8.2)
```

This is a hard check in `@tauri-apps/cli` 2.11.x that compares installed npm
packages against the Cargo.lock-resolved crates. It fires on **every**
`tauri build` invocation — including `build.yml`'s `pnpm tauri build
--no-bundle` desktop matrix, i.e. main CI's desktop job was already red
before this slice (the fork/main merge bumped the npm side to
`@tauri-apps/plugin-dialog@2.8.2` while `Cargo.lock` still pinned
`tauri-plugin-dialog 2.7.3`).

Fix (both directions tried, minimal cascade chosen):

1. `cargo update -p tauri-plugin-dialog --precise 2.8.3` — aligns the crate
   with npm 2.8.x. Cascade (semver-guaranteed by the resolver): `tauri`
   2.11.2→2.12.4, `tauri-runtime`/`tauri-runtime-wry` →2.12.2, `wry`
   0.55→0.57, `tray-icon` 0.23→0.25, `window-vibrancy` 0.6→0.8,
   `webview2-com` 0.38→0.39, `tauri-plugin-fs` 2.5→2.7, plus transitive
   cleanup (brotli 9, cfb 0.14, `cargo_toml` 1.0, toml 0.9 chain removed).
   107+/195− in Cargo.lock — a *scoped* update, not wholesale `cargo update`.
2. That cascade then tripped the second check pair (`tauri` crate 2.12.4 vs
   `@tauri-apps/api` 2.11.0), so `pnpm up @tauri-apps/api` → 2.12.3 (same
   major/minor as the crate). `pnpm up` pins the specifier → package.json now
   reads `"@tauri-apps/api": "^2.12.3"` — kept deliberately: the old lax
   `^2` specifier is what let the npm side float to 2.8.2 while the crates
   idled at 2.7.3, so the pin is the recurrence guard.

Alternative considered and rejected: downgrading `@tauri-apps/plugin-dialog`
to 2.7.x — moves the frontend *backwards* to satisfy a stale lock, and the
Phase 13 save-as dialog code was written against 2.8.

`tauri-plugin-opener` (2.5.4 both sides) was already aligned. With 1+2 done,
`tauri build` proceeds past the check into real compilation.

**Side effect**: this repair also unbreaks `build.yml`'s desktop job on main.

## 2. CI release workflow — `.github/workflows/release.yml`

Trigger: `push: tags: ["v*"]` only. Two jobs:

- **`bundle` (matrix, 3 legs)** — setup steps mirror `build.yml` exactly
  (Linux GTK/WebKit apt list, `dtolnay/rust-toolchain@stable`,
  `pnpm/action-setup` + `setup-node` 20 with pnpm cache on
  `pf2e-companion/pnpm-lock.yaml`, `Swatinem/rust-cache` keyed
  `release-${{ matrix.os }}`, `pnpm install --frozen-lockfile`). Then:

  | os | command | artifacts |
  |---|---|---|
  | ubuntu-latest | `pnpm tauri build --bundles deb,appimage` | `bundle/deb/*.deb`, `bundle/appimage/*.AppImage` |
  | windows-latest | `pnpm tauri build --bundles msi` | `bundle/msi/*.msi` |
  | macos-latest | `pnpm tauri build --bundles dmg` | `bundle/dmg/*.dmg` |

  `--bundles` scopes each leg to its own formats (no NSIS download on
  Windows, no rpm/AppImage tooling on macOS). `tauri.conf.json`'s
  `bundle.targets: "all"` stays untouched. Each leg hands its bundles to the
  next job via `actions/upload-artifact` (`if-no-files-found: error` so a
  bundling regression is loud).

- **`publish`** — `actions/download-artifact` (merge-multiple → `dist/`) then
  `softprops/action-gh-release@v2` with `files: dist/**` and
  `fail_on_unmatched_files: true`.

**Why `action-gh-release` over bare `upload-artifact` as the channel**:
friends-beta testers are non-technical — release assets download from the
Releases page *without a GitHub login* and *never expire*; workflow artifacts
need a logged-in session and age out (default 90 days). The two-phase shape
(one publish job, not one `action-gh-release` step per matrix leg) avoids the
concurrent-release-creation race and puts every platform's installer on a
single release. upload-artifact is used only as an internal handoff.

Unsigned by construction: no signing identities exist in CI env, so Tauri
skips codesign/notarization on macOS and msi signing on Windows.

## 3. README — Install section

New `## Install` right after the intro blockquote. Per-platform first-launch:

- **Windows msi**: SmartScreen → "More info → Run anyway".
- **macOS dmg**: right-click → Open → Open (Gatekeeper; Apple-silicon only in
  beta).
- **Linux deb**: `sudo apt install ./pf2e-companion_*.deb`.
- **Linux AppImage**: `chmod +x` + run; `--appimage-extract-and-run` fallback
  for FUSE-less distros.

Also states the no-account/no-telemetry posture and links the release
workflow. A maintainer note records that Linux bundles were produced and
verified locally at `src-tauri/target/release/bundle/`.

## 4. Local build evidence (this machine, x86_64 Linux)

`pnpm tauri build` (full bundle set, `EXIT=0`):

| bundle | path (under `src-tauri/target/release/bundle/`) | bytes | magic |
|---|---|---|---|
| deb | `deb/pf2e-companion_0.1.0_amd64.deb` | 19,266,246 | Debian binary package 2.0; control metadata correct (`Depends: libwebkit2gtk-4.1-0, libgtk-3-0`) |
| rpm | `rpm/pf2e-companion-0.1.0-1.x86_64.rpm` | 19,267,137 | RPM v3.0 (free byproduct of `targets: "all"`) |
| AppImage | `appimage/pf2e-companion_0.1.0_amd64.AppImage` | 97,311,224 | ELF 64-bit x86-64 (bundles WebKitGTK, hence the size) |

Environment notes for the next machine:

- Real GTK/WebKit dev packages were present (no pkg-config shim needed);
  `docs/STATUS.md`'s gtk-shim note is stale for this host.
- `APPIMAGE_EXTRACT_AND_RUN=1` was set in the env that worked (export it on
  FUSE-less hosts so linuxdeploy runs; harmless otherwise).
- No system packages installed, no sudo used. linuxdeploy/appimagetool were
  fetched by the Tauri bundler itself.
- The deb/rpm/AppImage sizes above are the sanity baseline for CI legs — a
  CI-produced Linux artifact wildly smaller or failing `file` magic means
  bundling silently degraded.

## 5. Test plan / gates

No new app code (lockfile alignment + CI + docs only), so the gate set is the
standing four plus YAML validation:

1. `corepack pnpm check` — **0 errors / 0 warnings** ✓
2. `corepack pnpm test` — **112/112 passed** ✓
3. `cargo test --all-features` — **WAIVED by orchestrator**: slice touches no
   `src-tauri/` source (only the `Cargo.lock` §1 alignment, verified via
   `git diff --name-only`), and the full dep tree + app compiled green in
   *both* release (bundle build, §4) and debug test profile (killed run had
   reached the final test binaries, tests shown passing before SIGKILL). The
   merged tree's cargo battery was green at 81a5a20.
4. `corepack pnpm build` — ✓ (also exercised as `beforeBuildCommand` inside
   the bundle build).
5. `python3 -c "import yaml; yaml.safe_load(...)"` on
   `.github/workflows/release.yml` — **parses** ✓ (PyYAML present on host).
6. Local bundle smoke: artifacts exist, `file` magic verified, deb control
   metadata inspected (§4). Windows/msi and macOS/dmg legs are CI-only —
   runner images carry their toolchains.

`pnpm e2e` / `e2e:live` deliberately not run: nothing frontend-behavioral
changed; orchestrator re-verifies at merge.

## 6. Acceptance criteria

1. `git tag v0.1.0 && git push --tags` produces a GitHub Release with a
   Linux `.deb` + `.AppImage`, a Windows `.msi`, and a macOS `.dmg` attached —
   all downloadable without a GitHub login.
2. README tells each platform's exact first-launch clicks for unsigned
   installers.
3. `build.yml` byte-identical to before this slice; no changes to
   `tauri.conf.json`, icons, or version (0.1.0).
4. All gates in §5 green; lockfile deltas limited to the §1 alignment
   (`Cargo.lock` scoped tauri closure, `pnpm-lock.yaml`
   `@tauri-apps/api` 2.11.0→2.12.3 only).
5. `pnpm tauri build` works locally end-to-end (§4 evidence) — previously a
   hard fail in <1s.

## 7. Risks / unknowns

- **CI leg variance**: macOS dmg leg on `macos-latest` is arm64-only (Intel
  Macs excluded from the beta — README says so). Windows leg: `--bundles msi`
  avoids the NSIS toolchain entirely. If GitHub's runners regress on
  linuxdeploy/AppImage tooling, the ubuntu leg may need
  `APPIMAGE_EXTRACT_AND_RUN=1` in env — add only if a run proves it.
- **Version-tag drift**: nothing enforces tag ↔ `tauri.conf.json` version
  match; a v0.2.0 tag with a 0.1.0 config ships mislabeled bundles.
  Deliberately out of scope (Tier 2 release plumbing).
- **Lockfile alignment blast radius**: the §1 cascade bumps the webview
  stack (wry 0.57, webview2-com 0.39). Semver-clean per the resolver and all
  gates pass, but real-device evenings (Tier 1 #1) are the true check; this
  repair was unavoidable — main CI's desktop job was already dead without it.
- **Self-hosted future**: if we later sign builds (Tier 2 #7), this workflow
  extends with secrets + `--config` overrides; the unsigned path documented
  here stays the fallback.
