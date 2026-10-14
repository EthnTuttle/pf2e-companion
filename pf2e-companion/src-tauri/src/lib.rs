//! pf2e-companion — Tauri 2 backend.
//!
//! Phase 0 scaffold:
//! 1. SQLite + FTS5 + sqlite-vec (loaded as a runtime extension).
//! 2. Schema migration on startup (see `db::SCHEMA_V1`).
//! 3. Vault watcher (notify-rs) — logs events; ingest in Phase 1.
//! 4. IPC: `search`, `list_entities`, `schema_version`.
//! 5. One smoke-test fixture seeded so `search("Hosts")` returns a row.
//!
//! Each subsequent phase fills in the rest of the spec at
//! `~/wiki/topics/pf2e-worldbuilding-tool/output/plan-cross-platform-pf2e-biblical-reference-2026-05-25.md`.

pub mod adventure;
mod commands;
pub mod content;
pub mod db;
pub mod dice;
pub mod eval;
mod foundry;
mod keystore;
pub mod llm;
pub mod llm_tools;
#[cfg(feature = "lan")]
pub mod lan;
#[cfg(feature = "p2p")]
pub mod net;
pub mod rag;
pub mod rules;
pub mod session;
mod vault;
mod vault_write;

use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,pf2e_companion_lib=debug".into()),
        )
        .try_init()
        .ok();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // App-data directory (per-platform; resolves correctly on iOS/Android too).
            let app_data = app
                .path()
                .app_data_dir()
                .expect("app data dir resolvable");
            std::fs::create_dir_all(&app_data).ok();

            let db_path = app_data.join("index.db");
            let db = db::Db::open(&db_path)?;
            db.seed_smoke_fixture()?;
            db.seed_reference_data()?;
            content::load_bundled_packs(&db)?;

            let db_arc = Arc::new(db);
            app.manage(db_arc.clone());

            // Phase 6 § B — LLM is off by default. The registry is created
            // unconfigured; `commands::llm_configure` populates it on user
            // opt-in.
            app.manage(Arc::new(llm::LlmRegistry::new()));

            // Phase 10 — Live Table. One active session per process.
            app.manage(Arc::new(session::SessionRegistry::new()));

            let vault_root = app_data.join("vault");
            std::fs::create_dir_all(&vault_root).ok();
            app.manage(Arc::new(vault_write::VaultRoot(vault_root.clone())));
            if let Err(e) = vault::spawn(vault_root, db_arc.clone()) {
                tracing::error!(error = %e, "failed to start vault watcher");
            }

            tracing::info!(db = %db_path.display(), "pf2e-companion ready");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::search,
            commands::list_entities,
            commands::schema_version,
            commands::lookup_alias,
            commands::lookup_miracle,
            commands::xp_budget,
            commands::creature_xp,
            commands::validate_statblock,
            commands::list_lenses,
            commands::import_foundry_pack,
            commands::get_entity,
            commands::list_campaigns,
            commands::create_campaign,
            commands::create_entity,
            commands::update_entity,
            commands::delete_entity,
            commands::add_relation,
            commands::delete_relation,
            commands::list_relations,
            commands::llm_status,
            commands::llm_configure,
            commands::llm_clear_config,
            commands::llm_chat,
            commands::rag_index_stats,
            commands::rag_reindex,
            commands::eval_load_suite,
            commands::eval_run,
            commands::session_host,
            commands::session_join_peer,
            commands::session_intent,
            commands::session_snapshot,
            commands::session_leave,
            commands::session_load,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
