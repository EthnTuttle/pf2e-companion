//! Phase 14 — first-run wizard (SHIPPING.md Tier 1 item 6).
//!
//! Covers the four contracts the unit tests in settings.rs can't reach:
//! 1. First-run detection — an empty default vault leaves the wizard pending.
//! 2. Existing-install migration — campaigns on disk auto-complete the
//!    wizard and persist settings.json (idempotently).
//! 3. `vault::clear_vault_index` scoping — vault rows + their FTS entries
//!    are dropped, reference rows survive.
//! 4. `effective_vault` + the change-detection helpers behind
//!    `wizard_complete`'s restart_required computation.

use pf2e_companion_lib::db::Db;
use pf2e_companion_lib::settings::{
    effective_vault, load, load_or_migrate, save, AppSettings,
};
use pf2e_companion_lib::vault;
use rusqlite::params;
use tempfile::TempDir;

#[test]
fn first_run_detected_when_default_vault_empty() {
    let app_data = TempDir::new().unwrap();
    // No settings.json, no campaigns → nothing to migrate from.
    let s = load_or_migrate(app_data.path());
    assert!(!s.wizard_completed, "fresh install should need the wizard");
    // Migration must not persist anything for a genuine first run.
    assert!(!app_data.path().join("settings.json").exists());
}

#[test]
fn existing_install_never_sees_wizard() {
    let app_data = TempDir::new().unwrap();
    std::fs::create_dir_all(app_data.path().join("vault").join("campaigns").join("c1"))
        .unwrap();
    let s = load_or_migrate(app_data.path());
    assert!(s.wizard_completed, "existing install should skip the wizard");

    // The migration persists (and is idempotent — second call re-reads it).
    let path = app_data.path().join("settings.json");
    assert!(path.exists());
    assert_eq!(load(app_data.path()).wizard_completed, true);
    assert_eq!(load_or_migrate(app_data.path()).wizard_completed, true);
}

/// Insert an entity row + refresh its FTS entry, mirroring the
/// vault.rs ingest path (entities insert then entities_fts refresh).
fn insert_entity(db: &Db, id: &str, source: &str, body: &str) {
    let conn = db.conn.lock().unwrap();
    conn.execute(
        r#"
        INSERT INTO entities
          (id, type, campaign_id, source, lens, license_provenance,
           frontmatter, body, body_text, statblock, file_path, mtime, hash)
        VALUES (?1, 'note', '_default', ?2, NULL, 'homebrew',
                ?3, ?4, ?4, NULL, ?5, 0, 'x')
        "#,
        params![
            id,
            source,
            format!(r#"{{"title":"{id}"}}"#),
            body,
            format!("{source}/{id}.md"),
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO entities_fts(rowid, title, body_text)
         SELECT rowid, COALESCE(json_extract(frontmatter, '$.title'), id), body_text
         FROM entities WHERE id = ?1",
        params![id],
    )
    .unwrap();
}

fn fts_count(db: &Db, needle: &str) -> i64 {
    let conn = db.conn.lock().unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM entities_fts WHERE entities_fts MATCH ?1",
        params![needle],
        |r| r.get(0),
    )
    .unwrap()
}

#[test]
fn vault_change_clears_derived_index_only() {
    let dir = TempDir::new().unwrap();
    let db = Db::open(&dir.path().join("index.db")).unwrap();
    insert_entity(&db, "v-one", "vault", "uniquevaultneedle one");
    insert_entity(&db, "r-one", "reference", "uniquereferenceneedle one");

    vault::clear_vault_index(&db);

    let conn = db.conn.lock().unwrap();
    let vault_rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entities WHERE source = 'vault'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let ref_rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entities WHERE source = 'reference'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    drop(conn);
    assert_eq!(vault_rows, 0, "vault row should be deleted");
    assert_eq!(ref_rows, 1, "reference row must survive");
    assert_eq!(fts_count(&db, "uniquevaultneedle"), 0, "vault FTS gone");
    assert_eq!(fts_count(&db, "uniquereferenceneedle"), 1, "reference FTS intact");
}

#[test]
fn effective_vault_and_change_detection() {
    let app_data = TempDir::new().unwrap();
    let custom = app_data.path().join("custom-vault");
    let custom_canon = custom.canonicalize().or_else(|_| {
        std::fs::create_dir_all(&custom).unwrap();
        custom.canonicalize()
    })
    .unwrap();

    // Default: no vault_path → <app_data>/vault.
    let mut s = AppSettings::default();
    assert_eq!(
        effective_vault(app_data.path(), &s),
        app_data.path().join("vault")
    );

    // Custom: vault_path wins.
    s.vault_path = Some(custom_canon.clone());
    assert_eq!(effective_vault(app_data.path(), &s), custom_canon);

    // Change detection — the mirror of wizard_complete's restart_required:
    // the next launch's effective vault vs. what the previous launch ran.
    s.last_run_vault = Some(app_data.path().join("vault"));
    let next_root = effective_vault(app_data.path(), &s);
    let changed = s.last_run_vault.as_ref() != Some(&next_root);
    assert!(changed, "different root should require a restart + index clear");

    // Same folder (canonicalized both sides — the re-pick case) → no change.
    s.last_run_vault = Some(custom_canon.clone());
    let same_root = effective_vault(app_data.path(), &s);
    assert_eq!(
        s.last_run_vault.as_ref(),
        Some(&same_root),
        "re-picking the running vault must not look like a change"
    );

    // Round-trip through save/load keeps the custom path.
    s.wizard_completed = true;
    save(app_data.path(), &s).unwrap();
    assert_eq!(load(app_data.path()).vault_path, Some(custom_canon));
}
