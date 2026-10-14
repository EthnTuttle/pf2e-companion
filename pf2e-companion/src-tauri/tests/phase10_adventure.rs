//! Phase 10 — adventure pack loader (CONTRACT § 6).
//!
//! Covers: the bundled widows-lamp adventure parses (3 scenes / 2
//! handouts / 3 NPCs, every sidecar `validate_statblock`-valid), via
//! both the inline fixture (always available to this test) and the
//! bundled content tree (authored by BUILDER-WEB; skipped with a notice
//! until those files land).

use include_dir::{include_dir, Dir};
use pf2e_companion_lib::adventure::{self, AdventureDef};
use pf2e_companion_lib::content;
use pf2e_companion_lib::db::Db;

static FIXTURE_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/tests/fixtures/adventure");

/// Parse the fixture adventure markdown's frontmatter into the mapping
/// shape `parse_adventure` expects (same as content.rs's combined map).
fn fixture_frontmatter() -> serde_yaml::Mapping {
    let raw = FIXTURE_DIR
        .get_file("widows-lamp.md")
        .expect("fixture adventure present")
        .contents_utf8()
        .unwrap();
    let (fm, _body) = split_frontmatter(raw);
    let value: serde_yaml::Value = serde_yaml::from_str(fm).unwrap();
    match value {
        serde_yaml::Value::Mapping(m) => m,
        other => panic!("fixture frontmatter not a mapping: {other:?}"),
    }
}

fn split_frontmatter(s: &str) -> (&str, &str) {
    let trimmed = s.trim_start_matches('\u{FEFF}');
    if !trimmed.starts_with("---") {
        return ("", trimmed);
    }
    let after = trimmed[3..].trim_start_matches(['\r', '\n']);
    match after.find("\n---") {
        Some(end) => (&after[..end], &after[end + 4..]),
        None => ("", trimmed),
    }
}

#[test]
fn fixture_adventure_parses_with_three_scenes_two_handouts_three_npcs() {
    let fm = fixture_frontmatter();
    let (_yaml, body) = {
        let raw = FIXTURE_DIR.get_file("widows-lamp.md").unwrap().contents_utf8().unwrap();
        let (fm, body) = split_frontmatter(raw);
        let _ = fm;
        (fm, body)
    };
    let def: AdventureDef =
        adventure::parse_adventure(&fm, body, &FIXTURE_DIR, "widows-lamp").unwrap();

    assert_eq!(def.id, "lewisian:adventure-widows-lamp");
    assert_eq!(def.title, "The Widow's Lamp");
    assert_eq!(def.scenes.len(), 3, "exactly 3 scenes");
    assert_eq!(def.handouts.len(), 2, "exactly 2 handouts");
    assert_eq!(def.npcs.len(), 3, "exactly 3 npc sidecars");
    assert!(def.body.contains("Run notes"));

    // Scene shapes: read-aloud on 1 + 3, encounter on 2.
    assert!(def.scenes[0].read_aloud.is_some());
    let enc = def.scenes[1].encounter.as_ref().expect("scene-2 encounter");
    assert_eq!(enc.difficulty, "moderate");
    assert_eq!(enc.npcs, vec!["debt-collector".to_string()]);

    // Handout visibility split.
    assert_eq!(def.handouts[0].visible_to, "all");
    assert_eq!(def.handouts[1].visible_to, "gm");

    // Validation is clean and every sidecar passes validate_statblock.
    assert!(adventure::validate_adventure(&def).is_empty());
    for npc in &def.npcs {
        let result = pf2e_companion_lib::rules::validate_statblock(&npc.statblock);
        assert!(result.valid, "npc `{}` invalid: {:?}", npc.slug, result.errors);
    }
}

#[test]
fn validation_flags_duplicates_and_bad_visibility() {
    let mut def =
        adventure::parse_adventure(&fixture_frontmatter(), "", &FIXTURE_DIR, "widows-lamp")
            .unwrap();
    // Duplicate a scene + handout, corrupt visibility.
    def.scenes.push(def.scenes[0].clone());
    def.handouts.push(def.handouts[0].clone());
    def.handouts[0].visible_to = "everyone".into();
    let problems = adventure::validate_adventure(&def);
    assert!(problems.iter().any(|p| p.contains("duplicate scene id")));
    assert!(problems.iter().any(|p| p.contains("duplicate handout id")));
    assert!(problems.iter().any(|p| p.contains("visible_to")));
}

#[test]
fn missing_sidecar_is_an_error() {
    let mut fm = fixture_frontmatter();
    fm.insert(
        serde_yaml::Value::from("npcs"),
        serde_yaml::Value::Sequence(vec![serde_yaml::Value::from("no-such-npc")]),
    );
    let err = adventure::parse_adventure(&fm, "", &FIXTURE_DIR, "widows-lamp").unwrap_err();
    assert!(
        err.to_string().contains("no-such-npc"),
        "expected missing-sidecar error, got: {err}"
    );
}

#[test]
fn sidecars_resolve_only_within_the_adventures_own_dir_and_stem() {
    // `sneaky` sidecars exist — but only in a decoy SUBDIRECTORY and
    // under a WRONG STEM. Exact-name resolution within the adventure's
    // own dir must find neither: parse succeeds (best-effort roster),
    // but validation FAILS because the rostered slug is unresolvable.
    let mut fm = fixture_frontmatter();
    set_scene2_roster(&mut fm, &["debt-collector", "sneaky"]);
    let def = adventure::parse_adventure(&fm, "", &FIXTURE_DIR, "widows-lamp").unwrap();
    assert!(
        !def.roster.contains_key("sneaky"),
        "decoy subdir / wrong-stem sidecars must NOT resolve: {:?}",
        def.roster.keys().collect::<Vec<_>>()
    );
    let problems = adventure::validate_adventure(&def);
    assert!(
        problems.iter().any(|p| p.contains("rosters npc `sneaky` without a sidecar")),
        "expected roster-sidecar validation failure, got: {problems:?}"
    );
    // The resolvable roster entry stays clean.
    assert!(!problems.iter().any(|p| p.contains("debt-collector")));
}

#[test]
fn roster_sidecar_in_own_dir_resolves_into_roster() {
    // A sidecar that IS anchored correctly (direct child, right stem)
    // resolves into `roster` without joining the top-level `npcs` list.
    let mut fm = fixture_frontmatter();
    set_scene2_roster(&mut fm, &["debt-collector", "lamplighter"]);
    let def = adventure::parse_adventure(&fm, "", &FIXTURE_DIR, "widows-lamp").unwrap();
    // `lamplighter` is a top-level npc — no duplicate roster entry.
    assert!(!def.roster.contains_key("lamplighter"));
    assert!(adventure::validate_adventure(&def).is_empty(), "fully resolvable roster");
}

/// Overwrite the fixture's scene-2 encounter roster.
fn set_scene2_roster(fm: &mut serde_yaml::Mapping, npcs: &[&str]) {
    let scenes = fm.get_mut(serde_yaml::Value::from("scenes")).unwrap();
    let serde_yaml::Value::Sequence(seq) = scenes else { panic!("scenes not a sequence") };
    let Some(serde_yaml::Value::Mapping(scene2)) = seq.get_mut(1) else {
        panic!("scene-2 missing")
    };
    let mut encounter = serde_yaml::Mapping::new();
    encounter.insert(
        serde_yaml::Value::from("difficulty"),
        serde_yaml::Value::from("moderate"),
    );
    encounter.insert(
        serde_yaml::Value::from("npcs"),
        serde_yaml::Value::Sequence(
            npcs.iter().map(|s| serde_yaml::Value::from(*s)).collect(),
        ),
    );
    scene2.insert(
        serde_yaml::Value::from("encounter"),
        serde_yaml::Value::Mapping(encounter),
    );
}

#[test]
fn bundled_widows_lamp_parses_via_content_loader() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(&tmp.path().join("phase10_adventure.db")).unwrap();
    let report = content::load_bundled_packs(&db).unwrap();
    assert!(report.packs_loaded >= 1);

    // BUILDER-WEB owns the bundled adventure content; until those files
    // land this probe is skipped (never fails the default test run).
    let has_adventures = {
        let conn = db.conn.lock().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM entities WHERE type = 'adventure'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
            > 0
    };
    if !has_adventures {
        eprintln!(
            "SKIP: no bundled adventures under data/content/lewisian/adventures/ yet \
             (BUILDER-WEB content); fixture coverage above is authoritative"
        );
        return;
    }

    let def = adventure::load_adventure_def(&db, "widows-lamp")
        .expect("bundled widows-lamp resolves by slug");
    assert_eq!(def.scenes.len(), 3, "3 scenes");
    assert_eq!(def.handouts.len(), 2, "2 handouts");
    assert_eq!(
        def.npcs.len(),
        4,
        "4 npcs (lamplighter, debt-collector, widow-mara, thug)"
    );
    // Every rostered slug resolves (thug sidecar landed) and the whole
    // def validates cleanly.
    let problems = adventure::validate_adventure(&def);
    assert!(
        problems.is_empty(),
        "bundled adventure must validate cleanly, got: {problems:?}"
    );
    for npc in &def.npcs {
        let result = pf2e_companion_lib::rules::validate_statblock(&npc.statblock);
        assert!(result.valid, "sidecar `{}` invalid: {:?}", npc.slug, result.errors);
    }

    // The full intended roster materializes as tokens — scene-2 rosters
    // `thug` twice, which must yield two distinct suffixed token ids.
    let mut doc = pf2e_companion_lib::session::SessionDoc::new_session(
        pf2e_companion_lib::session::SessionMeta {
            id: "bundled-roster".into(),
            name: "T".into(),
            gm_id: "gm".into(),
            created_at: 0,
            adventure_slug: None,
        },
    );
    doc.apply_adventure(&def, "widows-lamp").unwrap();
    let snap = doc.snapshot().unwrap();
    for token_id in ["npc-debt-collector-0", "npc-thug-0", "npc-thug-1"] {
        assert!(
            snap.scene.tokens.contains_key(token_id),
            "roster token `{token_id}` missing (tokens: {:?})",
            snap.scene.tokens.keys().collect::<Vec<_>>()
        );
    }
}
