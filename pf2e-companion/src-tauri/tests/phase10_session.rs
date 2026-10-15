//! Phase 10 — session doc + authority (CONTRACT § 6).
//!
//! Covers: save/load round-trip byte-stability; the authority matrix
//! negative cases (cross-player `update_sheet`, unowned `move_token`,
//! player `end_session`); and `apply_intent` → PatchEvent mapping for
//! every Intent variant of § 3.

use pf2e_companion_lib::adventure::AdventureDef;
use pf2e_companion_lib::session::{
    CharacterSheet, ChatKind, Intent, PatchEventPayload, Role, SessionDoc, SessionMeta,
};

fn meta() -> SessionMeta {
    SessionMeta {
        id: "test-session".to_string(),
        name: "Friday Table".to_string(),
        gm_id: "gm-peer".to_string(),
        created_at: 1_700_000_000_000,
        adventure_slug: None,
    }
}

fn sheet(hp: i64) -> CharacterSheet {
    CharacterSheet {
        level: 1,
        class: "Fighter".to_string(),
        hp,
        max_hp: 18,
        ac: 18,
        conditions: vec!["frightened".to_string()],
        notes: "shield raised".to_string(),
    }
}

fn gm(doc: &mut SessionDoc, intent: Intent) -> Vec<PatchEventPayload> {
    doc.apply_intent("gm-peer", Role::Gm, &intent)
        .unwrap()
        .into_iter()
        .map(|e| e.payload)
        .collect()
}

fn player(doc: &mut SessionDoc, intent: Intent) -> Vec<PatchEventPayload> {
    doc.apply_intent("p1", Role::Player, &intent)
        .unwrap()
        .into_iter()
        .map(|e| e.payload)
        .collect()
}

#[test]
fn save_load_round_trip_is_byte_stable() {
    let mut doc = SessionDoc::new_session(meta());
    gm(&mut doc, Intent::Chat { text: "hello table".into() });
    player(
        &mut doc,
        Intent::CreateCharacter { name: "Ruth".into(), color: "#fff".into(), character: sheet(18) },
    );
    let bytes = doc.save();

    let mut loaded = SessionDoc::load(&bytes).unwrap();
    let bytes2 = loaded.save();
    assert_eq!(bytes, bytes2, "save → load → save must be byte-stable");

    // And the snapshot survives.
    assert_eq!(doc.snapshot().unwrap(), loaded.snapshot().unwrap());
}

#[test]
fn session_id_survives_load() {
    let mut doc = SessionDoc::new_session(meta());
    gm(&mut doc, Intent::Chat { text: "x".into() });
    let loaded = SessionDoc::load(&doc.save()).unwrap();
    assert_eq!(loaded.session_id(), "test-session");
    assert_eq!(loaded.snapshot().unwrap().meta.name, "Friday Table");
}

// === Authority matrix negative cases ====================================

#[test]
fn player_cannot_update_another_players_sheet() {
    let mut doc = SessionDoc::new_session(meta());
    player(
        &mut doc,
        Intent::CreateCharacter { name: "Ruth".into(), color: "#fff".into(), character: sheet(18) },
    );
    let err = doc
        .apply_intent(
            "p2",
            Role::Player,
            &Intent::UpdateSheet {
                peer_id: Some("p1".into()),
                patch: Default::default(),
            },
        )
        .unwrap_err();
    assert!(
        err.to_string().to_lowercase().contains("forbidden"),
        "cross-player update_sheet must be Forbidden, got: {err}"
    );
    // State unchanged.
    let snap = doc.snapshot().unwrap();
    assert_eq!(snap.players["p1"].character.as_ref().unwrap().hp, 18);
}

#[test]
fn player_cannot_move_a_token_they_do_not_own() {
    let mut doc = SessionDoc::new_session(meta());
    gm(
        &mut doc,
        Intent::AddToken {
            token: pf2e_companion_lib::session::Token {
                token_id: "t1".into(),
                owner: "gm-peer".into(),
                entity_id: None,
                pc: None,
                x: 1.0,
                y: 2.0,
                label: "ogre".into(),
                image: String::new(),
            },
        },
    );
    let err = doc
        .apply_intent("p1", Role::Player, &Intent::MoveToken { token_id: "t1".into(), x: 9.0, y: 9.0 })
        .unwrap_err();
    assert!(err.to_string().to_lowercase().contains("forbidden"));
    // Token unmoved.
    assert_eq!(doc.snapshot().unwrap().scene.tokens["t1"].x, 1.0);
    // …but the owner may move it.
    gm(&mut doc, Intent::MoveToken { token_id: "t1".into(), x: 3.0, y: 4.0 });
    assert_eq!(doc.snapshot().unwrap().scene.tokens["t1"].x, 3.0);
}

#[test]
fn player_cannot_end_session() {
    let mut doc = SessionDoc::new_session(meta());
    let err = doc
        .apply_intent("p1", Role::Player, &Intent::EndSession)
        .unwrap_err();
    assert!(err.to_string().to_lowercase().contains("forbidden"));
    // GM can.
    let events = gm(&mut doc, Intent::EndSession);
    assert!(matches!(events.last(), Some(PatchEventPayload::SessionEnded)));
}

#[test]
fn player_secret_roll_is_stripped() {
    let mut doc = SessionDoc::new_session(meta());
    let events = player(
        &mut doc,
        Intent::RollDice { expr: "1d20".into(), secret: true },
    );
    // The roll lands in chat but is NOT flagged secret for a player.
    match &events[0] {
        PatchEventPayload::ChatAppended { entry } => {
            assert_eq!(entry.kind, ChatKind::Roll);
            assert!(entry.roll.is_some());
            assert!(
                !entry.text.to_lowercase().contains("secret"),
                "player secret flag must be stripped: {}",
                entry.text
            );
        }
        other => panic!("expected ChatAppended, got {other:?}"),
    }
    // GM secret rolls ARE flagged — and carry NO numbers in the doc
    // (the numeric result is acked out-of-band via `take_last_roll`).
    let events = gm(&mut doc, Intent::RollDice { expr: "1d20".into(), secret: true });
    match &events[0] {
        PatchEventPayload::ChatAppended { entry } => {
            assert!(entry.text.to_lowercase().contains("secret"));
            assert!(
                entry.roll.is_none(),
                "secret GM roll must not store numbers in the doc"
            );
            assert_eq!(entry.text, "(secret GM roll)");
        }
        other => panic!("expected ChatAppended, got {other:?}"),
    }
    let roll = doc.take_last_roll().expect("tracked roll for the host ack");
    assert_eq!(roll.expr, "1d20");
    assert!((1..=20).contains(&roll.total));
    assert_eq!(roll.rolls.len(), 1);
    // Consumed exactly once.
    assert!(doc.take_last_roll().is_none());
}

// === Chat seq (CONTRACT-10_5 § 3) ========================================

#[test]
fn chat_seq_monotonic_and_survives_save_load() {
    let mut doc = SessionDoc::new_session(meta());
    // Two rolls on the same tick: both kept, distinct monotonic seqs,
    // and the seqs are distributed in the derived patches.
    let e1 = gm(&mut doc, Intent::RollDice { expr: "1d20".into(), secret: false });
    let e2 = gm(&mut doc, Intent::RollDice { expr: "1d20".into(), secret: false });
    match (&e1[0], &e2[0]) {
        (
            PatchEventPayload::ChatAppended { entry: a },
            PatchEventPayload::ChatAppended { entry: b },
        ) => {
            assert_eq!(a.seq, Some(1), "first entry gets seq 1");
            assert_eq!(b.seq, Some(2), "second entry gets seq 2");
        }
        other => panic!("expected ChatAppended patches, got {other:?}"),
    }
    let snap = doc.snapshot().unwrap();
    assert_eq!(snap.chat.len(), 2, "same-tick entries both kept");
    assert_eq!(
        snap.chat.iter().map(|e| e.seq).collect::<Vec<_>>(),
        vec![Some(1), Some(2)],
        "seqs are monotonic in the snapshot"
    );

    // Reload: seqs persist and the counter continues.
    let mut loaded = SessionDoc::load(&doc.save()).unwrap();
    let snap = loaded.snapshot().unwrap();
    assert_eq!(
        snap.chat.iter().map(|e| e.seq).collect::<Vec<_>>(),
        vec![Some(1), Some(2)],
        "seqs survive save/load"
    );
    let e3 = gm(&mut loaded, Intent::Chat { text: "after reload".into() });
    match &e3[0] {
        PatchEventPayload::ChatAppended { entry } => {
            assert_eq!(entry.seq, Some(3), "counter continues after reload");
        }
        other => panic!("expected ChatAppended, got {other:?}"),
    }
}

// === apply_intent → PatchEvent mapping for every § 3 variant ==========

#[test]
fn create_character_maps_to_player_upserted() {
    let mut doc = SessionDoc::new_session(meta());
    let events = player(
        &mut doc,
        Intent::CreateCharacter { name: "Ruth".into(), color: "#ff0000".into(), character: sheet(18) },
    );
    assert_eq!(events.len(), 1);
    match &events[0] {
        PatchEventPayload::PlayerUpserted { peer_id, player } => {
            assert_eq!(peer_id, "p1");
            assert_eq!(player.name, "Ruth");
            assert_eq!(player.color, "#ff0000");
            assert_eq!(player.character.as_ref().unwrap().hp, 18);
        }
        other => panic!("expected PlayerUpserted, got {other:?}"),
    }
}

#[test]
fn update_sheet_maps_to_player_upserted() {
    let mut doc = SessionDoc::new_session(meta());
    player(
        &mut doc,
        Intent::CreateCharacter { name: "Ruth".into(), color: "#fff".into(), character: sheet(18) },
    );
    let events = player(
        &mut doc,
        Intent::UpdateSheet {
            peer_id: None,
            patch: pf2e_companion_lib::session::SheetPatch {
                hp: Some(12),
                conditions: Some(vec!["dying".into()]),
                ..Default::default()
            },
        },
    );
    assert_eq!(events.len(), 1);
    match &events[0] {
        PatchEventPayload::PlayerUpserted { player, .. } => {
            assert_eq!(player.character.as_ref().unwrap().hp, 12);
            assert_eq!(player.character.as_ref().unwrap().max_hp, 18); // untouched
            assert_eq!(player.character.as_ref().unwrap().conditions, vec!["dying".to_string()]);
        }
        other => panic!("expected PlayerUpserted, got {other:?}"),
    }
    // GM may patch any player's sheet.
    let events = gm(
        &mut doc,
        Intent::UpdateSheet {
            peer_id: Some("p1".into()),
            patch: pf2e_companion_lib::session::SheetPatch {
                hp: Some(18),
                ..Default::default()
            },
        },
    );
    assert!(matches!(events[0], PatchEventPayload::PlayerUpserted { .. }));
}

#[test]
fn chat_and_roll_map_to_chat_appended() {
    let mut doc = SessionDoc::new_session(meta());
    let events = player(&mut doc, Intent::Chat { text: "I search the body.".into() });
    match &events[0] {
        PatchEventPayload::ChatAppended { entry } => {
            assert_eq!(entry.kind, ChatKind::Chat);
            assert_eq!(entry.text, "I search the body.");
            assert_eq!(entry.from, "p1");
            assert!(entry.ts > 0);
        }
        other => panic!("expected ChatAppended, got {other:?}"),
    }

    let events = gm(&mut doc, Intent::RollDice { expr: "2d6+3".into(), secret: false });
    match &events[0] {
        PatchEventPayload::ChatAppended { entry } => {
            assert_eq!(entry.kind, ChatKind::Roll);
            let roll = entry.roll.as_ref().unwrap();
            assert_eq!(roll.expr, "2d6+3");
            assert_eq!(roll.rolls.len(), 2);
            assert_eq!(roll.total, roll.kept.iter().map(|&v| v as i64).sum::<i64>() + 3);
        }
        other => panic!("expected ChatAppended, got {other:?}"),
    }
    // Bad expressions are Invalid, not Forbidden.
    let err = doc
        .apply_intent("gm-peer", Role::Gm, &Intent::RollDice { expr: "bananas".into(), secret: false })
        .unwrap_err();
    assert!(err.to_string().to_lowercase().contains("invalid"));
}

#[test]
fn initiative_intents_map_to_initiative() {
    let mut doc = SessionDoc::new_session(meta());
    let events = gm(&mut doc, Intent::SetInitiative { order: vec!["p1".into(), "gm-peer".into()] });
    match &events[0] {
        PatchEventPayload::Initiative { order, active_idx, round } => {
            assert_eq!(order, &vec!["p1".to_string(), "gm-peer".to_string()]);
            assert_eq!(*active_idx, 0);
            assert_eq!(*round, 1);
        }
        other => panic!("expected Initiative, got {other:?}"),
    }
    let events = gm(&mut doc, Intent::AdvanceInitiative);
    match &events[0] {
        PatchEventPayload::Initiative { active_idx, .. } => assert_eq!(*active_idx, 1),
        other => panic!("expected Initiative, got {other:?}"),
    }
    // Wrap-around bumps the round.
    gm(&mut doc, Intent::AdvanceInitiative);
    let snap = doc.snapshot().unwrap();
    assert_eq!(snap.init.active_idx, 0);
    assert_eq!(snap.init.round, 2);
    // Advancing with no order is Invalid.
    let mut fresh = SessionDoc::new_session(meta());
    let err = fresh
        .apply_intent("gm-peer", Role::Gm, &Intent::AdvanceInitiative)
        .unwrap_err();
    assert!(err.to_string().to_lowercase().contains("invalid"));
}

#[test]
fn token_intents_map_to_upserted_and_removed() {
    let mut doc = SessionDoc::new_session(meta());
    let token = pf2e_companion_lib::session::Token {
        token_id: "t1".into(),
        owner: "p1".into(),
        entity_id: None,
        pc: Some("Ruth".into()),
        x: 1.0,
        y: 1.0,
        label: "Ruth".into(),
        image: String::new(),
    };
    let events = gm(&mut doc, Intent::AddToken { token: token.clone() });
    match &events[0] {
        PatchEventPayload::TokenUpserted { token: t } => assert_eq!(t, &token),
        other => panic!("expected TokenUpserted, got {other:?}"),
    }
    let events = player(&mut doc, Intent::MoveToken { token_id: "t1".into(), x: 5.5, y: 6.5 });
    match &events[0] {
        PatchEventPayload::TokenUpserted { token } => {
            assert_eq!(token.x, 5.5);
            assert_eq!(token.y, 6.5);
        }
        other => panic!("expected TokenUpserted, got {other:?}"),
    }
    let events = gm(&mut doc, Intent::RemoveToken { token_id: "t1".into() });
    assert!(matches!(&events[0], PatchEventPayload::TokenRemoved { token_id } if token_id == "t1"));
    assert!(doc.snapshot().unwrap().scene.tokens.is_empty());
    // Removing an unknown token is Invalid.
    let err = doc
        .apply_intent("gm-peer", Role::Gm, &Intent::RemoveToken { token_id: "nope".into() })
        .unwrap_err();
    assert!(err.to_string().to_lowercase().contains("invalid"));
}

#[test]
fn set_scene_maps_to_scene_set() {
    let mut doc = SessionDoc::new_session(meta());
    let events = gm(
        &mut doc,
        Intent::SetScene {
            grid: pf2e_companion_lib::session::Grid { w: 30, h: 20, size: 64 },
            background_entity_id: Some("lewisian:plane-heaven".into()),
        },
    );
    match &events[0] {
        PatchEventPayload::SceneSet { scene } => {
            assert_eq!(scene.grid.w, 30);
            assert_eq!(scene.grid.size, 64);
            assert_eq!(scene.background_entity_id.as_deref(), Some("lewisian:plane-heaven"));
        }
        other => panic!("expected SceneSet, got {other:?}"),
    }
}

fn fixture_def() -> AdventureDef {
    AdventureDef {
        id: "lewisian:adventure-widows-lamp".into(),
        title: "The Widow's Lamp".into(),
        scenes: vec![
            pf2e_companion_lib::adventure::Scene {
                id: "scene-1".into(),
                title: "The Lamplit Street".into(),
                read_aloud: Some("Rain slicks the cobbles.".into()),
                encounter: None,
            },
            pf2e_companion_lib::adventure::Scene {
                id: "scene-2".into(),
                title: "The Debt Collector".into(),
                read_aloud: None,
                encounter: Some(pf2e_companion_lib::adventure::Encounter {
                    difficulty: "moderate".into(),
                    npcs: vec!["debt-collector".into()],
                }),
            },
        ],
        handouts: vec![
            pf2e_companion_lib::adventure::HandoutDef {
                id: "letter".into(),
                title: "A Collector's Letter".into(),
                visible_to: "all".into(),
                body: None,
                image: None,
                image_alt: None,
            },
            pf2e_companion_lib::adventure::HandoutDef {
                id: "ledger".into(),
                title: "The Household Ledger".into(),
                visible_to: "gm".into(),
                body: None,
                image: None,
                image_alt: None,
            },
        ],
        npcs: vec![pf2e_companion_lib::adventure::NpcRef {
            slug: "debt-collector".into(),
            statblock: serde_json::json!({
                "name": "The Debt Collector", "level": 1, "type": "npc",
                "license_provenance": "homebrew"
            }),
        }],
        roster: Default::default(),
        body: "# Run notes".into(),
    }
}

#[test]
fn load_adventure_populates_slug_tokens_handouts_and_readaloud() {
    let mut doc = SessionDoc::new_session(meta());
    let def = fixture_def();
    let events = doc.apply_adventure(&def, "widows-lamp").unwrap();
    let payloads: Vec<_> = events.iter().map(|e| e.payload.clone()).collect();

    assert!(
        payloads.iter().any(|p| matches!(
            p,
            PatchEventPayload::AdventureLoaded { slug } if slug == "widows-lamp"
        )),
        "expected AdventureLoaded, got {payloads:?}"
    );
    assert!(
        payloads.iter().any(|p| matches!(
            p,
            PatchEventPayload::TokenUpserted { token } if token.label == "The Debt Collector"
        )),
        "expected encounter npc token, got {payloads:?}"
    );
    assert!(
        payloads.iter().any(|p| matches!(
            p,
            PatchEventPayload::ChatAppended { entry }
                if entry.kind == ChatKind::System
                    && entry.text.contains("Rain slicks the cobbles.")
        )),
        "expected scene-1 read-aloud system chat, got {payloads:?}"
    );

    let snap = doc.snapshot().unwrap();
    assert_eq!(snap.meta.adventure_slug.as_deref(), Some("widows-lamp"));
    assert_eq!(snap.handouts.len(), 2);
    assert_eq!(snap.handouts["ledger"].visible_to, "gm");
    assert!(snap.scene.tokens.contains_key("npc-debt-collector-0"));

    // Imagery v1: the token's art path is stamped exactly when the promoted
    // portrait is bundled; otherwise the field stays empty (UI fallback).
    let art = "lewisian/art/adventures/widows-lamp/npc-debt-collector.webp";
    if pf2e_companion_lib::images::bundled_image_exists(art) {
        assert_eq!(snap.scene.tokens["npc-debt-collector-0"].image, art);
    } else {
        assert!(snap.scene.tokens["npc-debt-collector-0"].image.is_empty());
    }
}

#[test]
fn reveal_handout_maps_to_handout_revealed() {
    let mut doc = SessionDoc::new_session(meta());
    let def = fixture_def();
    doc.apply_adventure(&def, "widows-lamp").unwrap();

    let events = gm(&mut doc, Intent::RevealHandout { handout_id: "ledger".into() });
    assert!(
        events.iter().any(|p| matches!(
            p,
            PatchEventPayload::HandoutRevealed { handout_id } if handout_id == "ledger"
        )),
        "expected HandoutRevealed, got {events:?}"
    );
    assert_eq!(doc.snapshot().unwrap().handouts["ledger"].visible_to, "all");
    // Reveal requires the GM.
    let err = doc
        .apply_intent("p1", Role::Player, &Intent::RevealHandout { handout_id: "ledger".into() })
        .unwrap_err();
    assert!(err.to_string().to_lowercase().contains("forbidden"));
}

#[test]
fn load_adventure_intent_sets_slug() {
    let mut doc = SessionDoc::new_session(meta());
    let events = gm(&mut doc, Intent::LoadAdventure { slug: "widows-lamp".into() });
    assert!(matches!(
        &events[0],
        PatchEventPayload::AdventureLoaded { slug } if slug == "widows-lamp"
    ));
    assert_eq!(
        doc.snapshot().unwrap().meta.adventure_slug.as_deref(),
        Some("widows-lamp")
    );
}

#[test]
fn end_session_maps_to_session_ended() {
    let mut doc = SessionDoc::new_session(meta());
    let events = gm(&mut doc, Intent::EndSession);
    // SessionEnded plus a player_upserted per roster entry (presence
    // cleared — characters stay).
    assert!(events.iter().any(|p| matches!(p, PatchEventPayload::SessionEnded)));
    assert!(events.iter().any(|p| matches!(
        p,
        PatchEventPayload::PlayerUpserted { player, .. } if !player.connected
    )));
}

#[test]
fn blank_doc_snapshot_uses_defaults() {
    // v1 forward-compat: a blank doc (pre-sync joiner) decodes with
    // defaults rather than failing.
    let doc = SessionDoc::blank();
    let snap = doc.snapshot().unwrap();
    assert!(snap.meta.id.is_empty());
    assert!(snap.chat.is_empty());
    assert_eq!(snap.scene.grid.w, 24);
}

// === Roster / presence (reconnect-preserves-sheet) ======================

#[test]
fn disconnect_keeps_roster_entry_and_reattach_restores_it() {
    let mut doc = SessionDoc::new_session(meta());
    player(
        &mut doc,
        Intent::CreateCharacter { name: "Ruth".into(), color: "#fff".into(), character: sheet(18) },
    );
    let peer_id = {
        let snap = doc.snapshot().unwrap();
        assert!(snap.players["p1"].connected);
        "p1".to_string()
    };

    // Disconnect: the roster entry survives (character included).
    doc.mark_disconnected(&peer_id);
    let snap = doc.snapshot().unwrap();
    let player = snap.players.get(&peer_id).expect("roster entry kept");
    assert!(!player.connected, "marked disconnected");
    assert!(player.character.is_some(), "character survives disconnect");

    // Reconnect under a NEW peer id but the same name → reattached to
    // the original identity, character intact.
    let effective = doc.join_player("brand-new-peer", "Ruth", "#0f0");
    assert_eq!(effective, peer_id, "same-name reconnect reattaches");
    let snap = doc.snapshot().unwrap();
    let player = snap.players.get(&peer_id).unwrap();
    assert!(player.connected);
    assert_eq!(player.character.as_ref().unwrap().hp, 18);
    // No duplicate roster entry was minted.
    assert!(!snap.players.contains_key("brand-new-peer"));

    // A different name does NOT reattach — fresh entry.
    let fresh = doc.join_player("another-peer", "Someone Else", "#00f");
    assert_eq!(fresh, "another-peer");
}

#[test]
fn save_load_round_trip_preserves_disconnected_roster() {
    let mut doc = SessionDoc::new_session(meta());
    player(
        &mut doc,
        Intent::CreateCharacter { name: "Ruth".into(), color: "#fff".into(), character: sheet(18) },
    );
    doc.mark_disconnected("p1");
    let loaded = SessionDoc::load(&doc.save()).unwrap();
    let snap = loaded.snapshot().unwrap();
    let player = snap.players.get("p1").expect("roster survives persistence");
    assert!(!player.connected);
    assert_eq!(player.character.as_ref().unwrap().hp, 18);
}

#[test]
fn end_session_clears_presence_but_keeps_characters() {
    let mut doc = SessionDoc::new_session(meta());
    player(
        &mut doc,
        Intent::CreateCharacter { name: "Ruth".into(), color: "#fff".into(), character: sheet(18) },
    );
    gm(&mut doc, Intent::EndSession);
    let snap = doc.snapshot().unwrap();
    assert!(!snap.players["p1"].connected, "presence cleared on end");
    assert!(!snap.players["gm-peer"].connected);
    assert!(
        snap.players["p1"].character.is_some(),
        "characters stay on the roster after end_session"
    );
}

#[test]
fn duplicate_roster_slugs_get_distinct_token_ids() {
    let mut doc = SessionDoc::new_session(meta());
    let mut def = fixture_def();
    // Two scenes both rostering the same slug must not collapse.
    def.scenes.push(pf2e_companion_lib::adventure::Scene {
        id: "scene-3".into(),
        title: "Rematch".into(),
        read_aloud: None,
        encounter: Some(pf2e_companion_lib::adventure::Encounter {
            difficulty: "low".into(),
            npcs: vec!["debt-collector".into()],
        }),
    });
    doc.apply_adventure(&def, "widows-lamp").unwrap();
    let snap = doc.snapshot().unwrap();
    assert!(
        snap.scene.tokens.contains_key("npc-debt-collector-0"),
        "tokens: {:?}",
        snap.scene.tokens.keys().collect::<Vec<_>>()
    );
    assert!(
        snap.scene.tokens.contains_key("npc-debt-collector-1"),
        "duplicate slug must get a suffixed token id"
    );
}

#[test]
fn roster_only_slugs_materialize_tokens_with_slug_label() {
    // A rostered slug with no sidecar still gets its token (labeled by
    // slug); a rostered slug WITH a sidecar uses the statblock name.
    let mut doc = SessionDoc::new_session(meta());
    let mut def = fixture_def();
    def.scenes[1].encounter.as_mut().unwrap().npcs.push("thug-1".into());
    def.roster.insert(
        "thug-1".into(),
        serde_json::json!({ "name": "Dockside Thug", "level": -1, "type": "npc" }),
    );
    def.scenes[1].encounter.as_mut().unwrap().npcs.push("mook".into());
    doc.apply_adventure(&def, "widows-lamp").unwrap();
    let snap = doc.snapshot().unwrap();
    assert_eq!(
        snap.scene.tokens["npc-thug-1-0"].label, "Dockside Thug",
        "roster sidecar name is used"
    );
    assert_eq!(
        snap.scene.tokens["npc-mook-0"].label, "mook",
        "sidecar-less roster slug falls back to the slug label"
    );
}
