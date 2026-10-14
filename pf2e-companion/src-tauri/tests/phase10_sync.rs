//! Phase 10 — automerge two-peer convergence (CONTRACT § 6).
//!
//! Two `SessionDoc`s exchange raw `automerge::sync::Message`s in an
//! in-process loop (no network, no features required) until both go
//! idle; snapshots must converge in both directions and PatchEvents
//! must be derived from the merge diff.

use pf2e_companion_lib::session::doc::{SessionEvent, SharedDoc};
use pf2e_companion_lib::session::{
    CharacterSheet, Intent, PatchEventPayload, Role, SessionDoc, SessionMeta,
};
use tokio::sync::broadcast;

fn meta(id: &str) -> SessionMeta {
    SessionMeta {
        id: id.to_string(),
        name: "Sync Table".to_string(),
        gm_id: "gm-peer".to_string(),
        created_at: 1_700_000_000_000,
        adventure_slug: None,
    }
}

fn sheet() -> CharacterSheet {
    CharacterSheet {
        level: 1,
        class: "Cleric".to_string(),
        hp: 16,
        max_hp: 16,
        ac: 16,
        conditions: vec![],
        notes: String::new(),
    }
}

/// Pump messages between the two docs until both sides go idle.
fn sync_to_idle(a: &SharedDoc, b: &SharedDoc, tx: &broadcast::Sender<SessionEvent>) {
    use automerge::sync::State;
    let mut state_a = State::new(); // A's state for its exchange with B
    let mut state_b = State::new(); // B's state for its exchange with A
    for _ in 0..100 {
        let mut progressed = false;
        if let Some(msg) = a.generate_sync_message(&mut state_a) {
            b.receive_sync_message(&mut state_b, msg, tx).unwrap();
            progressed = true;
        }
        if let Some(msg) = b.generate_sync_message(&mut state_b) {
            a.receive_sync_message(&mut state_a, msg, tx).unwrap();
            progressed = true;
        }
        if !progressed {
            return;
        }
    }
    panic!("sync did not converge within 100 rounds");
}

#[tokio::test(flavor = "multi_thread")]
async fn two_docs_converge_both_directions() {
    // Host doc with content.
    let host = SharedDoc::new(SessionDoc::new_session(meta("sync-session")));
    host.with(|d| {
        d.apply_intent(
            "gm-peer",
            Role::Gm,
            &Intent::Chat { text: "welcome to the table".into() },
        )
        .unwrap();
        d.apply_intent(
            "p1",
            Role::Player,
            &Intent::CreateCharacter {
                name: "Ruth".into(),
                color: "#fff".into(),
                character: sheet(),
            },
        )
        .unwrap();
    });

    // Joiner starts blank.
    let joiner = SharedDoc::new(SessionDoc::blank());
    let (tx, mut rx) = broadcast::channel(128);
    sync_to_idle(&host, &joiner, &tx);

    // Snapshots converge.
    let host_snap = host.with(|d| d.snapshot().unwrap());
    let joiner_snap = joiner.with(|d| d.snapshot().unwrap());
    assert_eq!(host_snap, joiner_snap);
    assert_eq!(joiner_snap.meta.id, "sync-session");
    assert_eq!(joiner_snap.chat.len(), 1);
    assert!(joiner_snap.players.contains_key("p1"));

    // The joiner's channel saw the derived patch events.
    let mut kinds = Vec::new();
    while let Ok(event) = rx.try_recv() {
        if let SessionEvent::Patch(p) = event {
            kinds.push(p.payload);
        }
    }
    assert!(
        kinds.iter().any(|p| matches!(p, PatchEventPayload::PlayerUpserted { peer_id, .. } if peer_id == "p1")),
        "expected PlayerUpserted in merge diff, got {kinds:?}"
    );
    assert!(
        kinds.iter().any(|p| matches!(p, PatchEventPayload::ChatAppended { .. })),
        "expected ChatAppended in merge diff, got {kinds:?}"
    );

    // Reverse direction: a change on the joiner propagates to the host.
    joiner.with(|d| {
        d.apply_intent("p1", Role::Player, &Intent::Chat { text: "hello from the joiner".into() })
            .unwrap();
    });
    sync_to_idle(&host, &joiner, &tx);
    let host_snap = host.with(|d| d.snapshot().unwrap());
    assert_eq!(host_snap.chat.len(), 2);
    assert_eq!(host_snap.chat[1].text, "hello from the joiner");
    assert_eq!(host.with(|d| d.snapshot().unwrap()), joiner.with(|d| d.snapshot().unwrap()));
}

#[tokio::test(flavor = "multi_thread")]
async fn save_bytes_from_converged_docs_are_equivalent() {
    // The canonical record: a host that saved, and a joiner that synced
    // the same changes, hold equivalent state.
    let host = SharedDoc::new(SessionDoc::new_session(meta("canonical")));
    host.with(|d| {
        d.apply_intent("gm-peer", Role::Gm, &Intent::Chat { text: "record".into() })
            .unwrap();
    });
    let joiner = SharedDoc::new(SessionDoc::blank());
    let (tx, _rx) = broadcast::channel(16);
    sync_to_idle(&host, &joiner, &tx);

    let host_saved = SessionDoc::load(&host.with(|d| d.save())).unwrap();
    let joiner_saved = SessionDoc::load(&joiner.with(|d| d.save())).unwrap();
    assert_eq!(host_saved.snapshot().unwrap(), joiner_saved.snapshot().unwrap());
    assert_eq!(joiner_saved.session_id(), "canonical");
}
