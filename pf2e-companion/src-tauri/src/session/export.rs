//! Session log rendering + export (SHIPPING.md Tier 1 item 5).
//!
//! Pure markdown rendering of a [`SessionSnapshot`] plus disk-export
//! helpers. The exported `.log.md` is a derived artifact — the canonical
//! `.automerge` doc is never mutated (files-canonical discipline).

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

use super::doc::{now_ms, SessionDoc};
use super::service::safe_segment;
use super::{ChatEntry, ChatKind, SessionSnapshot};

/// ISO-8601 UTC from unix millis (numeric fallback when out of range).
fn iso_utc(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_else(|| ms.to_string())
}

/// `HH:MM` UTC from unix millis.
fn hhmm(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|dt| dt.format("%H:%M").to_string())
        .unwrap_or_else(|| "??:??".into())
}

/// Render a session transcript as markdown. Pure — no I/O.
pub fn render_session_log(
    snap: &SessionSnapshot,
    campaign: Option<&str>,
    exported_at_ms: i64,
) -> String {
    let meta = &snap.meta;
    let mut out = String::new();

    // Header.
    out.push_str(&format!("# Session: {}\n\n", meta.name));
    out.push_str(&format!("- Session ID: {}\n", meta.id));
    out.push_str(&format!("- Campaign: {}\n", campaign.unwrap_or("—")));
    out.push_str(&format!(
        "- Adventure: {}\n",
        meta.adventure_slug.as_deref().unwrap_or("—")
    ));
    out.push_str(&format!("- Started: {}\n", iso_utc(meta.created_at)));
    out.push_str(&format!("- Exported: {}\n\n", iso_utc(exported_at_ms)));

    // Table (GM first, then the remaining roster).
    out.push_str("## Table\n\n");
    if let Some(gm) = snap.players.get(&meta.gm_id) {
        out.push_str(&player_line(&gm.name, gm.character.as_ref()));
        out.push_str(" (GM)\n");
    }
    for (peer_id, player) in &snap.players {
        if *peer_id == meta.gm_id {
            continue;
        }
        out.push_str(&player_line(&player.name, player.character.as_ref()));
        out.push('\n');
    }
    out.push('\n');

    // Transcript (doc order — seq/ts sorted).
    out.push_str("## Transcript\n\n");
    let mut chat = snap.chat.clone();
    chat.sort_by(|a, b| (a.seq.unwrap_or(0), a.ts).cmp(&(b.seq.unwrap_or(0), b.ts)));
    if chat.is_empty() {
        out.push_str("_(no messages)_\n\n");
    } else {
        for entry in &chat {
            out.push_str(&render_chat_line(entry));
            out.push('\n');
        }
        out.push('\n');
    }

    // Handouts — revealed first, GM-only under a separate sub-list.
    out.push_str("## Handouts\n\n");
    if snap.handouts.is_empty() {
        out.push_str("_(none)_\n");
    } else {
        for (id, h) in snap.handouts.iter().filter(|(_, h)| h.visible_to == "all") {
            out.push_str(&format!("- {} (entity {}) — revealed\n", id, h.entity_id));
        }
        let hidden: Vec<_> =
            snap.handouts.iter().filter(|(_, h)| h.visible_to != "all").collect();
        if !hidden.is_empty() {
            out.push_str("\nGM-only (unrevealed):\n");
            for (id, h) in hidden {
                out.push_str(&format!("- {} (entity {})\n", id, h.entity_id));
            }
        }
    }

    out
}

fn player_line(name: &str, character: Option<&super::CharacterSheet>) -> String {
    match character {
        Some(c) => format!("- {} — {} {}", name, c.class, c.level),
        None => format!("- {} (no character)", name),
    }
}

/// One transcript line, mirroring the play page's dice line
/// (`expr = total` + per-die breakdown).
fn render_chat_line(entry: &ChatEntry) -> String {
    let time = hhmm(entry.ts);
    match entry.kind {
        ChatKind::Roll => match &entry.roll {
            Some(roll) => {
                let mut line = format!(
                    "**[{}] {}** rolls `{}` → **{}**",
                    time, entry.from, roll.expr, roll.total
                );
                if !roll.rolls.is_empty() {
                    let dice: Vec<String> =
                        roll.rolls.iter().map(|d| d.to_string()).collect();
                    line.push_str(&format!(" (dice: {})", dice.join(", ")));
                    if !roll.kept.is_empty() && roll.kept.len() < roll.rolls.len() {
                        let kept: Vec<String> =
                            roll.kept.iter().map(|d| d.to_string()).collect();
                        line.push_str(&format!(" kept: {}", kept.join(", ")));
                    }
                }
                line
            }
            // Legacy tolerance: pre-10.x roll entries without a record.
            None => format!("**[{}] {}:** {}", time, entry.from, entry.text),
        },
        ChatKind::Chat => format!("**[{}] {}:** {}", time, entry.from, entry.text),
        ChatKind::System => format!("*[{}] {}*", time, entry.text),
    }
}

/// One saved session as shown in the lobby list.
#[derive(Debug, Clone, Serialize)]
pub struct SavedSessionSummary {
    pub session_id: String,
    pub campaign: String,
    pub name: String,
    pub created_at: i64,
    pub adventure_slug: Option<String>,
    pub ended: bool,
    pub saved_at: i64,
}

/// Locate `campaigns/*/sessions/{session_id}.automerge`; the campaign is
/// recovered from the directory name (`SessionInfo` carries no campaign).
fn find_session_file(vault: &Path, session_id: &str) -> Result<(String, PathBuf)> {
    let campaigns = vault.join("campaigns");
    let entries = std::fs::read_dir(&campaigns)
        .with_context(|| format!("read {}", campaigns.display()))?;
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let Some(campaign) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let candidate =
            entry.path().join("sessions").join(format!("{session_id}.automerge"));
        if candidate.is_file() {
            return Ok((campaign, candidate));
        }
    }
    bail!("no saved session {session_id:?} under {}", campaigns.display())
}

/// Export a session log. `dest: None` writes
/// `sessions/{session_id}.log.md` next to the canonical `.automerge`
/// (the vault-default fallback). Atomic tmp+rename, like
/// `SessionService::save_now`.
pub fn export_log(
    vault: &Path,
    session_id: &str,
    dest: Option<&Path>,
) -> Result<PathBuf> {
    safe_segment(session_id)?;
    let (campaign, doc_path) = find_session_file(vault, session_id)?;
    let bytes =
        std::fs::read(&doc_path).with_context(|| format!("read {}", doc_path.display()))?;
    let doc = SessionDoc::load(&bytes)?;
    let snap = doc.snapshot()?;
    let text = render_session_log(&snap, Some(&campaign), now_ms());

    let target = match dest {
        Some(p) => PathBuf::from(p),
        None => doc_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| vault.to_path_buf())
            .join(format!("{session_id}.log.md")),
    };
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create {}", parent.display()))?;
    }
    let tmp = target.with_extension("md.tmp");
    std::fs::write(&tmp, &text)
        .with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, &target)
        .with_context(|| format!("rename {}", target.display()))?;
    Ok(target)
}

/// Every saved session across all campaigns, most recently saved first.
/// Corrupt/unloadable files are skipped with a warning — a listing must
/// never fail because one save went bad.
pub fn list_saved(vault: &Path) -> Result<Vec<SavedSessionSummary>> {
    let mut out = Vec::new();
    let campaigns = vault.join("campaigns");
    if !campaigns.is_dir() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(&campaigns)
        .with_context(|| format!("read {}", campaigns.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let campaign = entry.file_name().to_string_lossy().into_owned();
        let sessions_dir = entry.path().join("sessions");
        if !sessions_dir.is_dir() {
            continue;
        }
        for file in std::fs::read_dir(&sessions_dir)
            .with_context(|| format!("read {}", sessions_dir.display()))?
        {
            let path = file?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("automerge") {
                continue;
            }
            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(error = %e, path = %path.display(), "unreadable session save; skipping");
                    continue;
                }
            };
            match summarize_save(&path, &bytes, &campaign) {
                Some(summary) => out.push(summary),
                None => continue,
            }
        }
    }
    out.sort_by(|a, b| b.saved_at.cmp(&a.saved_at));
    Ok(out)
}

/// Meta + mtime for one `.automerge` save; `None` (warned) when corrupt.
fn summarize_save(path: &Path, bytes: &[u8], campaign: &str) -> Option<SavedSessionSummary> {
    let doc = match SessionDoc::load(bytes) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "corrupt session save; skipping");
            return None;
        }
    };
    let snap = match doc.snapshot() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "unreadable session meta; skipping");
            return None;
        }
    };
    let saved_at = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    Some(SavedSessionSummary {
        session_id: snap.meta.id,
        campaign: campaign.to_string(),
        name: snap.meta.name,
        created_at: snap.meta.created_at,
        adventure_slug: snap.meta.adventure_slug,
        ended: snap.meta.ended,
        saved_at,
    })
}

