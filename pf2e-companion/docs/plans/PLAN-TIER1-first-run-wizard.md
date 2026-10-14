# PLAN — Tier 1 slice: "First-run wizard" (SHIPPING.md item 6)

Status: PLANNED (not implemented). Source of truth: `docs/SHIPPING.md` § Tier 1
item 6: "3 screens (vault location, lens pick, LLM optional / BYO-key)
replacing today's hunt-through-Settings." Includes the vault-root
configurability the earlier plans deferred. Extends Phase 13 (tree clean at
`7ebac15`); nothing in prior contracts changes.

## 1. The slice

A one-time `/onboarding` wizard (3 screens) shown only on genuine first run,
plus the vault-root configurability it implies: a persisted app-settings
file, first-run/existing-install detection, and a restart-based vault
re-point. Existing installs never see the wizard.

**Persistence — hand-rolled `settings.json`, NO new dep.** The project
hand-rolls every persistence layer (markdown vault, automerge saves,
`include_str!` seeds); a 3-key JSON file doesn't justify `tauri-plugin-store`
(dep + capability + lockfile churn + the platform-scoping risk the Phase 12
review hit on the dialog plugin). A ~90-line `settings.rs` with serde +
atomic tmp+rename (the exact `SessionService::save_now` pattern,
service.rs:224-237) is consistent and testable.

**Vault re-point — restart, not live.** The watcher (`vault::spawn`,
vault.rs:57) is one-shot at launch with no shutdown handle, the SQLite index
is derived from whatever root it watched, and `VaultRoot` is managed at
startup (lib.rs:78-83). Live re-pointing means a cancellable watcher +
mid-run index surgery; `AppHandle::restart()` after the wizard writes the
setting is the minimal correct path. Only a custom vault choice restarts.

## 2. Implementation steps

All paths relative to repo root `pf2e-companion/`. **No new cargo or npm
deps** — the dialog plugin (folder pick) is already installed and
`dialog:default` (which includes the `open` command) is in
`src-tauri/capabilities/default.json`. Doc schema and wire formats untouched.

### Step 1 — `src-tauri/src/settings.rs` (new, ~110 lines)

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)] pub vault_path: Option<PathBuf>,  // None = default
    #[serde(default)] pub wizard_completed: bool,
    #[serde(default)] pub last_run_vault: Option<PathBuf>, // change detection
}
```

- `pub fn settings_path(app_data: &Path) -> PathBuf` — `app_data/settings.json`.
- `pub fn load(app_data: &Path) -> AppSettings` — missing/corrupt file →
  defaults (corrupt logs a `tracing::warn!` through the Phase 13 hub).
- `pub fn save(app_data: &Path, &AppSettings) -> Result<()>` — atomic
  `.tmp` + rename, parent dirs created.
- `pub fn effective_vault(app_data: &Path, s: &AppSettings) -> PathBuf` —
  `s.vault_path.clone().unwrap_or_else(|| app_data.join("vault"))`.
- `pub fn load_or_migrate(app_data: &Path) -> AppSettings` — load, then the
  **existing-install guard**: if `!wizard_completed` AND the default vault
  has content (`read_dir(<app_data>/vault/campaigns)` yields ≥1 entry —
  campaigns are created by the first entity or first hosted session), set
  `wizard_completed = true` and save. Post-upgrade installs therefore never
  fire the wizard. Returns the (possibly migrated) settings.
- `pub fn validate_vault_path(raw: &str) -> Result<PathBuf, String>` —
  reject empty / relative / `"/"`; `create_dir_all` (so a new folder works);
  `canonicalize()`; return the normalized path. Errors are user-facing
  strings.
- `src-tauri/src/lib.rs`: `pub mod settings;` in the module list.

Effort: ~1.5 h including tests.

### Step 2 — Startup wiring (`src-tauri/src/lib.rs`, ~20 lines changed)

Inside `setup` (lines 53-87), between the DB open and the vault section:

1. `let settings = settings::load_or_migrate(&app_data);`
2. `let vault_root = settings::effective_vault(&app_data, &settings);`
3. **Vault-change index reset**: if `settings.last_run_vault` is `Some(p)`
   and `p != vault_root` → `vault::clear_vault_index(&db)` (Step 3) — keeps
   the derived index honest (old root's rows would otherwise linger in
   search). Then update `last_run_vault = Some(vault_root)` and save if it
   changed (no write on ordinary boots).
4. Replace lines 78-83: `create_dir_all(&vault_root)`, manage
   `vault_write::VaultRoot(vault_root.clone())`, `vault::spawn(vault_root,
   db_arc)` — unchanged code, new root. Also `app.manage(Arc::new(settings))`
   for the IPC commands.
5. Extend the ready log (line 85): `tracing::info!(vault =
   %vault_root.display(), …)` — lands in diagnostics for free.

Effort: ~30 min.

### Step 3 — `pub fn clear_vault_index(db: &Db)` (`src-tauri/src/vault.rs`, ~15 lines)

Mirror the existing delete path (vault.rs:210, 229) but keyed by source:

```sql
DELETE FROM entities_fts WHERE rowid IN (SELECT rowid FROM entities WHERE source = 'vault');
DELETE FROM entities WHERE source = 'vault';
```

Bundled/reference rows (`source = 'reference'`) untouched. Public so
`tests/phase14_wizard.rs` can exercise it against a real `Db`.

Effort: ~15 min.

### Step 4 — IPC commands (`src-tauri/src/commands.rs`, ~55 lines)

After `diagnostics_get`, same style:

- `wizard_status(settings: State<'_, Arc<AppSettings>>) -> Result<WizardStatus, String>`
  — `{ needed: !settings.wizard_completed }` (serde struct in commands.rs or
  settings.rs).
- `wizard_validate_vault(path: String) -> Result<String, String>` —
  `settings::validate_vault_path(&path)`; returns the normalized path for
  immediate UI feedback.
- `wizard_complete(vault_path: Option<String>, app, settings-state, vault:
  State<'_, Arc<VaultRoot>>) -> Result<WizardCompleteResult, String>`:
  validate/normalize `vault_path` (if given), persist
  `{vault_path, wizard_completed: true}`, return
  `{ restart_required: bool }` — true iff the effective vault differs from
  the `VaultRoot` this process is currently running.
- `app_restart(app: tauri::AppHandle) -> Result<(), String>` — `app.restart()`
  (never returns on success; named generically — future settings changes
  will reuse it).

Register all four in `generate_handler!` (lib.rs, after
`commands::diagnostics_get`).

Effort: ~45 min.

### Step 5 — Lens persistence (`src/lib/lens.svelte.ts`, `src/routes/+layout.svelte`, ~10 lines)

`lensState.active` currently resets to `"lewisian"` every launch
(lens.svelte.ts:4-8) — nothing persists it. The wizard's lens screen needs
it to stick:

- lens.svelte.ts: initialize `active` from
  `localStorage.getItem("pf2e.lens") ?? "lewisian"` (same key convention as
  `pf2e.playerName` in the table lobby); add
  `export function setActiveLens(id: string)` that sets state + writes
  localStorage.
- `+layout.svelte`: `$effect(() => { localStorage.setItem("pf2e.lens",
  lensState.active); })` — persists topbar picker changes too (free fix for
  the existing picker).

Effort: ~15 min.

### Step 6 — `src/lib/wizard.svelte.ts` (new, ~60 lines)

Testable wizard state (mirrors `session.svelte.ts` module shape):

- `export const wizardState = $state({ step: 0, vaultPath: null as string |
  null, useDefaultVault: true, lens: "lewisian", llmChoice: "skip" as
  "skip" | "ollama" | "anthropic", error: null })`.
- `export function next()/back()` — step 0→1→2, guards (vault must be chosen
  or default; lens always has a default).
- `export function shouldRedirectToWizard(pathname: string, needed: boolean):
  boolean` — pure: `needed && !pathname.startsWith("/onboarding")`.
- `export function wizardProgress(step: number): string` — pure, for the
  "Step 1 of 3 · Vault" header.

Effort: ~30 min.

### Step 7 — `src/routes/onboarding/+page.svelte` (new, ~180 lines)

One route, three internal screens driven by `wizardState.step` (simpler
than nested routes — no step-routing edge cases; the task's "e.g.
/onboarding/*" is satisfied by the internal stepper). Layout follows the
`/settings` card + hero pattern (phase 13's
`src/routes/settings/+page.svelte`):

- **Screen 1 — Vault location**: explains the vault ("campaigns, entities,
  and session saves live here as plain markdown"); radio between **Keep
  default** (pre-selected; shows `WizardStatus.default_vault`) and **Choose
  folder…** → `open({ directory: true })` (dynamic
  `import("@tauri-apps/plugin-dialog")`) → `wizardValidateVault` for
  immediate feedback; `null` (cancel) keeps the previous choice.
- **Screen 2 — Lens pick**: `ensureLensesLoaded()` then radio list from
  `lensState.manifests` (label + description — the same data the topbar
  picker uses); selection calls `setActiveLens` immediately (persists via
  Step 5).
- **Screen 3 — LLM optional / BYO-key**: "Off by default — the app is fully
  usable without it" copy (matches `/settings/llm` posture). Radio: **Skip**
  (default) / **Ollama** (model, `http://localhost:11434`) / **Anthropic**
  (model + API key). On **Finish**: if a provider was chosen, call the
  existing `llmConfigure(config, apiKey)` (keychain persistence for free,
  commands.rs:432); no reindex UI here — link to `/settings/llm`.
- **Finish flow**: `wizardComplete(vaultPath)` → if `restart_required`:
  show "Vault location set — restarting…" then fire-and-forget
  `appRestart()` (the process exits) — else `goto("/")`.
- **Skip wizard** link (footer, every screen): `wizardComplete(null)` →
  `goto("/")`; never fires again.
- A11y: fieldset/legend + labeled radios (EntityEditor-cleanup
  conventions); `pnpm check` stays 0/0.

`WizardStatus` carries `default_vault: String` (from `VaultRoot`) so screen
1 can show it — one serde field, no new command.

Effort: ~1.5 h.

### Step 8 — First-run routing (`src/routes/+layout.svelte`, ~10 lines)

In the existing `onMount` (line 9): `const status = await wizardStatus()` →
`if (shouldRedirectToWizard(page.url.pathname, status.needed))
goto("/onboarding")`. Runs once per app load; the pathname guard prevents
loops; `/onboarding` itself is excluded. `ssr = false` already
(`src/routes/+layout.ts`), so this is client-only. Lens/campaign loads stay
as-is (harmless behind the wizard).

Effort: ~15 min.

### Step 9 — ipc.ts + README (~25 lines)

`src/lib/ipc.ts` (after `diagnosticsGet`):

```ts
export interface WizardStatus { needed: boolean; default_vault: string; }
export interface WizardCompleteResult { restart_required: boolean; }
export const wizardStatus = () => invoke<WizardStatus>("wizard_status");
export const wizardValidateVault = (path: string) => invoke<string>("wizard_validate_vault", { path });
export const wizardComplete = (vaultPath: string | null) => invoke<WizardCompleteResult>("wizard_complete", { vaultPath });
export const appRestart = () => invoke<void>("app_restart");
```

README: "What works" gains a First-run wizard bullet; "Run your first game"
step 0 mentions it; note the vault is relocatable via the wizard and the
setting lives at `<app_data>/settings.json`.

Effort: ~20 min.

## 3. Test plan

**Rust — unit tests in `settings.rs`** (`#[cfg(test)]`, tempdirs):

1. `defaults_when_file_missing` / `corrupt_file_falls_back_to_defaults`.
2. `save_load_round_trip` (atomic write, `#[serde(default)]` tolerates
   unknown keys — forward compat).
3. `validate_vault_path` — accepts absolute tempdir; creates a missing
   subdirectory and returns the canonicalized path; rejects empty,
   relative, and `"/"`.

**Rust — new `src-tauri/tests/phase14_wizard.rs`** (integration, real
`Db::open` on tempdirs, no Tauri):

4. `first_run_detected_when_default_vault_empty` — tempdir app_data, no
   campaigns → `load_or_migrate` → `wizard_completed == false`.
5. `existing_install_never_sees_wizard` — tempdir app_data with
   `vault/campaigns/c1/` present → `load_or_migrate` → `wizard_completed ==
   true` AND `settings.json` written (idempotent on second call).
6. `vault_change_clears_derived_index_only` — `Db::open` tempdir, insert one
   `source='vault'` and one `source='reference'` row via `db.conn` (+FTS),
   `vault::clear_vault_index(&db)` → vault row and its FTS entry gone,
   reference row intact.
7. `effective_vault_and_change_detection` — `vault_path: None` → default;
   `Some(custom)` → custom; `last_run_vault` differs → restart-required
   logic (mirror of the `wizard_complete` computation, tested via the
   settings helpers it calls).

**Vitest — new `src/lib/wizard.test.ts`** (colocated, matches
`session.svelte.test.ts` conventions):

- `shouldRedirectToWizard`: false when `!needed`; false for
  `/onboarding`, `/onboarding/anything`; true for `/`, `/table` when needed.
- `wizardProgress`: labels for steps 0/1/2.
- Step machine via the store: 0→1→2 on `next()`; `back()` reverses; error
  set when a typed vault path fails validation (mock `$lib/ipc`).

**Gates** (sandbox): `cd src-tauri &&
PKG_CONFIG_PATH=/tmp/opencode/gtk-shim/pkgconfig cargo test` (default) and
`cargo test --all-features` 0-fail;
`~/.nvm/versions/node/v24.12.0/bin/pnpm test`; `pnpm check` (0 errors / 0
warnings); `pnpm build`. No env-gating — no network, no GUI automation. The
xvfb smoke (`PF2E_GUI_SMOKE=1`) should be run once manually before push
since startup order changed — it catches setup panics exactly like this.

## 4. Acceptance criteria

1. Fresh install (empty app-data): first launch lands on `/onboarding`;
   the three screens flow vault → lens → LLM; Finish returns to `/` and the
   wizard never reappears (reloads included).
2. Existing install (vault with campaigns, no settings.json): first launch
   after upgrade goes straight to `/` — wizard never fires (auto-migrated
   `settings.json` with `wizard_completed: true`).
3. Choosing a custom vault folder: validated immediately; on Finish the
   app announces the restart, relaunches, and the vault watcher + `/vault`
   CRUD + session saves operate under the new root; search no longer
   returns entities from the old root (derived index cleared once).
4. "Keep default" and "Skip wizard" both complete without restart and
   suppress the wizard permanently.
5. Lens chosen in the wizard survives relaunch (localStorage), and the
   topbar lens picker's manual changes now persist too.
6. Choosing Ollama/Anthropic in screen 3 configures the provider exactly as
   `/settings/llm` does (same keychain storage, `llmStatus` reports
   configured); Skip leaves LLM off.
7. No wizard on browser LAN clients (no Tauri IPC — the redirect call fails
   silently into `/`); direct-nav to `/onboarding` without IPC degrades to
   a "restart the app" notice, not a crash.
8. All gates green: `cargo test` (+`--all-features`), `pnpm test`, `pnpm
   check` (0/0), `pnpm build`. No lockfile changes; `settings.json` is the
   only new on-disk artifact.

## 5. Risks / unknowns

- **`app.restart()` in dev mode** restarts the dev binary — test the Finish
  flow in `pnpm tauri dev` once during implementation; if flaky in dev it's
  still correct in release builds (document any dev-only quirk in README's
  Develop section, don't code around it).
- **Restart kills the webview mid-invoke**: `appRestart()` is fire-and-forget
  (never awaited); the UI shows its "restarting…" state BEFORE calling.
- **Migration heuristic breadth**: "campaigns dir non-empty" misses a user
  who only ever browsed bundled content (empty vault) — but that user IS
  effectively first-run for vault purposes; showing them the wizard is
  correct, not a bug. Document the reasoning in `load_or_migrate`'s doc
  comment.
- **Canonicalization**: `validate_vault_path` canonicalizes (symlinks, case
  on macOS); `restart_required` must compare canonicalized paths or a
  re-pick of the same folder could spuriously restart.
- **Mobile folder picker**: `open({ directory: true })` on Android/iOS is
  the flakiest dialog surface (same class as Phase 12's save picker).
  Mitigation: "Keep default" is pre-selected and is the expected mobile
  path; if `open()` throws, show the error and stay on default. Screen copy
  notes custom locations are mainly for desktop.
- **`localStorage` in the Tauri webview** persists per-app and is already
  relied on (`pf2e.playerName`); lens persistence rides the same mechanism.
- **Test-inserted FTS rows**: test 6 inserts via `db.conn` directly; mirror
  the `entities_fts` refresh pattern from vault.rs:208-214 so the FTS
  assertion is meaningful.
- **Startup-order regressions**: settings load sits between DB open and
  watcher spawn; a panic there kills the app before the window — keep
  `load_or_migrate` infallible (defaults on every error path) and run the
  xvfb smoke once (see Gates).
- **No new deps**: `AppHandle::restart` is core Tauri 2; serde/std/notify
  already present. `Cargo.lock`/`pnpm-lock.yaml` must not change.
