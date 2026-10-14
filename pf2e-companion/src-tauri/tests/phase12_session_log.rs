//! Phase 12 — session log export (SHIPPING.md Tier 1 item 5).
//!
//! Covers: markdown rendering (header/roster/transcript/handouts, legacy
//! roll tolerance); disk export (explicit destination, vault-default
//! fallback, unsafe-id rejection); the saved-session listing (meta,
//! campaign, ended, mtime sort, corrupt-file tolerance); and the `ended`
//! flag round-trip (CONTRACT-10_5 § 8 additive-field pattern).

use pf2e_companion_lib::session::export::{
    export_log, list_saved, render_session_log, SavedSessionSummary,
};
use pf2e_companion_lib::session::{
    CharacterSheet, ChatEntry, ChatKind, Intent, PatchEventPayload, Role, SessionDoc,
    SessionMeta,
};
use std::path::Path;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

fn meta() -> SessionMeta {
    SessionMeta {
        id: "s1".to_string(),
        name: "Friday Table".to_string(),
        gm_id: "gm-peer".to_string(),
        created_at: 1_700_000_000_000,
        adventure_slug: None,
        ended: false,
    }
}

fn sheet() -> CharacterSheet {
    CharacterSheet {
        level: 3,
        class: "Fighter".to_string(),
        hp: 30,
        max_hp: 30,
        ac: 19,
        conditions: vec![],
        notes: String::new(),
    }
}

fn gm(doc: &mut SessionDoc, intent: Intent) -> Vec<PatchEventPayload> {
    doc.apply_intent("gm-peer", Role::Gm, &intent)
        .unwrap()
        .into_iter()
        .map(|e| e.payload)
        .collect()
}

fn player(doc: &mut SessionDoc, intent: Intent) {
    doc.apply_intent("p1", Role::Player, &intent).unwrap();
}

/// Persist `doc` as `campaigns/<campaign>/sessions/<id>.automerge`.
fn save_in(vault: &Path, campaign: &str, doc: &mut SessionDoc) {
    let dir = vault.join("campaigns").join(campaign).join("sessions");
    std::fs::create_dir_all(&dir).unwrap();
    let id = doc.session_id().to_string();
    std::fs::write(dir.join(format!("{id}.automerge")), doc.save()).unwrap();
}

fn set_mtime(path: &Path, ago_ms: u64) {
    let t = SystemTime::now() - Duration::from_millis(ago_ms);
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(t)
        .unwrap();
}

// === Rendering ===========================================================

#[test]
fn render_log_includes_header_roster_chat_rolls_and_handouts() {
    let mut doc = SessionDoc::new_session(meta());
    gm(&mut doc, Intent::Chat { text: "welcome to the table".into() });
    player(
        &mut doc,
        Intent::CreateCharacter { name: "Ruth".into(), color: "#fff".into(), character: sheet() },
    );
    player(&mut doc, Intent::Chat { text: "ready!".into() });
    let events = gm(&mut doc, Intent::RollDice { expr: "1d20+7".into(), secret: false });
    // Recover the roll total from the derived chat_appended patch — rolls
    // are random, never assert a literal.
    let roll_total = events
        .iter()
        .find_map(|e| match e {
            PatchEventPayload::ChatAppended { entry } if entry.roll.is_some() => {
                Some(entry.roll.as_ref().unwrap().total)
            }
            _ => None,
        })
        .expect("roll produced a chat entry");
    // Register + reveal a handout (registration is adventure-driven, so
    // seed the doc map directly, mirroring doc.rs).
    doc.with_doc(|d| {
        use automerge::transaction::Transactable;
        use automerge::ReadDoc;
        let handouts = d
            .get(automerge::ROOT, "handouts")
            .unwrap()
            .unwrap()
            .1;
        let h = d.put_object(&handouts, "letter", automerge::ObjType::Map).unwrap();
        d.put(&h, "entity_id", "widows-lamp-letter").unwrap();
        d.put(&h, "visible_to", "gm").unwrap();
    });
    gm(&mut doc, Intent::RevealHandout { handout_id: "letter".into() });

    let snap = doc.snapshot().unwrap();
    let out = render_session_log(&snap, Some("friday"), 1_700_000_060_000);

    assert!(out.contains("# Session: Friday Table"));
    assert!(out.contains("- Session ID: s1"));
    assert!(out.contains("- Campaign: friday"));
    assert!(out.contains("welcome to the table"));
    assert!(out.contains("ready!"));
    assert!(out.contains("`1d20+7`"));
    assert!(out.contains(&format!("→ **{}**", roll_total)));
    assert!(out.contains("Ruth — Fighter 3"), "class/level roster line");
    assert!(out.contains("letter (entity widows-lamp-letter) — revealed"));
    assert!(out.contains("2023-11-14T22:13:20Z"), "started ISO timestamp");
    assert!(out.contains("2023-11-14T22:14:20Z"), "exported ISO timestamp (60 s later)");
}

#[test]
fn render_log_handles_empty_and_legacy_entries() {
    let mut snap = SessionDoc::new_session(meta()).snapshot().unwrap();
    let out = render_session_log(&snap, None, 0);
    assert!(out.contains("## Transcript"));
    assert!(out.contains("_(no messages)_"));

    // Legacy roll entry without a numeric record renders text-only.
    snap.chat.push(ChatEntry {
        ts: 1_700_000_000_500,
        from: "gm-peer".into(),
        kind: ChatKind::Roll,
        text: "1d20+7 = 14".into(),
        roll: None,
        seq: None,
    });
    let out = render_session_log(&snap, None, 0);
    assert!(out.contains("1d20+7 = 14"));
    assert!(!out.contains("rolls `1d20+7`"));
}

// === Disk export =========================================================

#[test]
fn export_to_explicit_destination_writes_only_there() {
    let vault = TempDir::new().unwrap();
    let mut doc = SessionDoc::new_session(meta());
    gm(&mut doc, Intent::Chat { text: "hello".into() });
    save_in(vault.path(), "friday", &mut doc);

    let dest = vault.path().join("elsewhere").join("my-log.md");
    let out = export_log(vault.path(), "s1", Some(&dest)).unwrap();
    assert_eq!(out, dest);
    let text = std::fs::read_to_string(&dest).unwrap();
    assert!(text.contains("hello"));
    // Nothing in the vault's sessions dir besides the canonical doc.
    let sessions_dir = vault.path().join("campaigns/friday/sessions");
    let names: Vec<String> = std::fs::read_dir(&sessions_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["s1.automerge".to_string()]);
}

#[test]
fn export_default_falls_back_to_vault_sessions_dir_idempotently() {
    let vault = TempDir::new().unwrap();
    let mut doc = SessionDoc::new_session(meta());
    save_in(vault.path(), "friday", &mut doc);

    let expected = vault
        .path()
        .join("campaigns/friday/sessions/s1.log.md");
    let first = export_log(vault.path(), "s1", None).unwrap();
    assert_eq!(first, expected);
    assert!(expected.is_file());
    gm(&mut doc, Intent::Chat { text: "more chat".into() });
    save_in(vault.path(), "friday", &mut doc);

    let second = export_log(vault.path(), "s1", None).unwrap();
    assert_eq!(second, expected, "re-export overwrites in place");
    let text = std::fs::read_to_string(&expected).unwrap();
    assert!(text.contains("more chat"), "second export reflects new content");
}

#[test]
fn export_log_rejects_unknown_and_unsafe_session_ids() {
    let vault = TempDir::new().unwrap();
    let mut doc = SessionDoc::new_session(meta());
    save_in(vault.path(), "friday", &mut doc);

    assert!(export_log(vault.path(), "nope", None).is_err(), "unknown id");
    assert!(export_log(vault.path(), "../evil", None).is_err(), "traversal");
    assert!(export_log(vault.path(), "a/b", None).is_err(), "nested path");
}

// === Saved-session listing ==============================================

#[test]
fn list_saved_returns_meta_campaign_ended_sorted_by_mtime() {
    let vault = TempDir::new().unwrap();

    // Two campaigns, three sessions; s2 carries an adventure + has ended.
    let mut doc_a = SessionDoc::new_session(SessionMeta { id: "s1".into(), ..meta() });
    gm(&mut doc_a, Intent::Chat { text: "a".into() });
    save_in(vault.path(), "friday", &mut doc_a);

    let mut doc_b = SessionDoc::new_session(SessionMeta { id: "s2".into(), ..meta() });
    gm(&mut doc_b, Intent::LoadAdventure { slug: "widows-lamp".into() });
    gm(&mut doc_b, Intent::EndSession);
    save_in(vault.path(), "saturday", &mut doc_b);

    let mut doc_c = SessionDoc::new_session(SessionMeta { id: "s3".into(), ..meta() });
    save_in(vault.path(), "saturday", &mut doc_c);

    // Deterministic mtimes: s1 oldest, s2 middle, s3 newest.
    set_mtime(&vault.path().join("campaigns/friday/sessions/s1.automerge"), 30_000);
    set_mtime(&vault.path().join("campaigns/saturday/sessions/s2.automerge"), 20_000);
    set_mtime(&vault.path().join("campaigns/saturday/sessions/s3.automerge"), 10_000);

    let saved = list_saved(vault.path()).unwrap();
    assert_eq!(saved.len(), 3);
    let ids: Vec<&str> = saved.iter().map(|s| s.session_id.as_str()).collect();
    assert_eq!(ids, vec!["s3", "s2", "s1"], "most recently saved first");

    let s2 = saved.iter().find(|s| s.session_id == "s2").unwrap();
    assert_eq!(s2.name, "Friday Table");
    assert_eq!(s2.campaign, "saturday");
    assert_eq!(s2.adventure_slug.as_deref(), Some("widows-lamp"));
    assert!(s2.ended);
    assert!(s2.saved_at > 0);
    let s1 = saved.iter().find(|s| s.session_id == "s1").unwrap();
    assert!(!s1.ended);
    assert_eq!(s1.adventure_slug, None);
}

#[test]
fn list_saved_skips_corrupt_files() {
    let vault = TempDir::new().unwrap();
    let dir = vault.path().join("campaigns/friday/sessions");
    std::fs::create_dir_all(&dir).unwrap();

    let mut doc = SessionDoc::new_session(meta());
    gm(&mut doc, Intent::Chat { text: "ok".into() });
    std::fs::write(dir.join("s1.automerge"), doc.save()).unwrap();
    std::fs::write(dir.join("garbage.automerge"), b"not an automerge doc").unwrap();
    std::fs::write(dir.join("note.txt"), b"ignored by extension filter").unwrap();

    let saved = list_saved(vault.path()).unwrap();
    let ids: Vec<&str> = saved.iter().map(|s: &SavedSessionSummary| s.session_id.as_str()).collect();
    assert_eq!(ids, vec!["s1"], "corrupt file skipped, valid one listed");
}

// === `ended` flag round-trip (CONTRACT-10_5 § 8 additive field) =========

#[test]
fn ended_flag_round_trips_through_save_load() {
    let mut doc = SessionDoc::new_session(meta());
    assert!(!doc.snapshot().unwrap().meta.ended);
    gm(&mut doc, Intent::EndSession);
    assert!(doc.snapshot().unwrap().meta.ended);

    let bytes = doc.save();
    let loaded = SessionDoc::load(&bytes).unwrap();
    assert!(loaded.snapshot().unwrap().meta.ended);
}

