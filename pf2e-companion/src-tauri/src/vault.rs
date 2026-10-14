//! Vault watcher + markdown ingestion.
//!
//! Phase 1 wires the file-watch loop into SQLite: a markdown file with YAML
//! frontmatter becomes one row in `entities`; a sibling `.statblock.json` is
//! attached as the row's `statblock` JSON. Plan § 4.1.

use crate::db::Db;
use anyhow::{anyhow, Context, Result};
use notify::{recommended_watcher, Event, EventKind, RecursiveMode, Watcher};
use rusqlite::params;
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;

const FRONTMATTER_DELIM: &str = "---";

#[derive(Debug, Default, Deserialize)]
struct Frontmatter {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default, rename = "type")]
    entity_type: Option<String>,
    #[serde(default)]
    lens: Option<String>,
    #[serde(default)]
    license_provenance: Option<String>,
    #[serde(default)]
    campaign_id: Option<String>,
    /// Catch-all for the rest of the frontmatter so we can round-trip it
    /// into `entities.frontmatter` as JSON.
    #[serde(flatten)]
    extras: serde_yaml::Mapping,
}

#[derive(Debug)]
struct Parsed {
    id: String,
    entity_type: String,
    campaign_id: String,
    lens: Option<String>,
    license_provenance: String,
    frontmatter_json: String,
    body: String,
    body_text: String,
    statblock_json: Option<String>,
    file_path: String,
    mtime: i64,
    hash: String,
}

/// Spin up a notify-rs watcher for the vault root and ingest every markdown
/// file found at startup, plus any subsequent file events.
pub fn spawn(vault_root: PathBuf, db: Arc<Db>) -> Result<()> {
    if !vault_root.exists() {
        std::fs::create_dir_all(&vault_root)?;
    }

    // Initial sweep — ingest whatever is already on disk.
    sweep(&vault_root, &db)?;

    let (tx, rx) = mpsc::channel();
    let mut watcher = recommended_watcher(tx)?;
    watcher.watch(&vault_root, RecursiveMode::Recursive)?;
    Box::leak(Box::new(watcher));

    let db_thread = db.clone();
    let root_thread = vault_root.clone();
    std::thread::spawn(move || {
        for ev in rx {
            match ev {
                Ok(event) => {
                    if let Err(e) = handle_event(&event, &root_thread, &db_thread) {
                        tracing::warn!(error = %e, "ingest error");
                    }
                }
                Err(e) => tracing::warn!(error = %e, "vault watcher error"),
            }
        }
    });

    tracing::info!(vault = %vault_root.display(), "vault watcher started");
    Ok(())
}

fn handle_event(event: &Event, vault_root: &Path, db: &Db) -> Result<()> {
    match event.kind {
        EventKind::Create(_) | EventKind::Modify(_) => {
            for path in &event.paths {
                if is_markdown(path) && !is_session_artifact(path) {
                    ingest_file(path, vault_root, db)?;
                }
            }
        }
        EventKind::Remove(_) => {
            for path in &event.paths {
                if is_markdown(path) && !is_session_artifact(path) {
                    delete_by_path(path, vault_root, db)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn sweep(vault_root: &Path, db: &Db) -> Result<()> {
    let mut count = 0usize;
    walk(vault_root, &mut |path: &Path| -> Result<()> {
        if is_markdown(path) && !is_session_artifact(path) {
            ingest_file(path, vault_root, db)?;
            count += 1;
        }
        Ok(())
    })?;
    tracing::info!(count, "vault initial sweep complete");
    Ok(())
}

fn walk(path: &Path, on_file: &mut dyn FnMut(&Path) -> Result<()>) -> Result<()> {
    if path.is_file() {
        on_file(path)?;
        return Ok(());
    }
    if !path.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let p = entry.path();
        // skip dotfiles + index.db neighbours
        if p.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with('.'))
            .unwrap_or(false)
        {
            continue;
        }
        walk(&p, on_file)?;
    }
    Ok(())
}

fn is_markdown(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("md") | Some("markdown")
    )
}

/// Session artifacts live under any `sessions/` directory (exported
/// `.log.md` transcripts). They are derived files with no vault
/// frontmatter — ingesting them would hard-reject and churn warnings.
fn is_session_artifact(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "sessions")
}

fn ingest_file(path: &Path, vault_root: &Path, db: &Db) -> Result<()> {
    let parsed = parse_file(path, vault_root)?;
    let conn = db.conn.lock().unwrap();
    let prov = &parsed.license_provenance;
    if !matches!(
        prov.as_str(),
        "orc" | "community-use" | "homebrew" | "proprietary"
    ) {
        return Err(anyhow!(
            "{}: invalid license_provenance `{prov}` — must be orc | community-use | homebrew | proprietary",
            parsed.file_path
        ));
    }
    conn.execute(
        r#"
        INSERT INTO entities
          (id, type, campaign_id, source, lens, license_provenance,
           frontmatter, body, body_text, statblock, file_path, mtime, hash)
        VALUES (?1, ?2, ?3, 'vault', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        ON CONFLICT(id) DO UPDATE SET
          type = excluded.type,
          campaign_id = excluded.campaign_id,
          lens = excluded.lens,
          license_provenance = excluded.license_provenance,
          frontmatter = excluded.frontmatter,
          body = excluded.body,
          body_text = excluded.body_text,
          statblock = excluded.statblock,
          file_path = excluded.file_path,
          mtime = excluded.mtime,
          hash = excluded.hash
        "#,
        params![
            parsed.id,
            parsed.entity_type,
            parsed.campaign_id,
            parsed.lens,
            parsed.license_provenance,
            parsed.frontmatter_json,
            parsed.body,
            parsed.body_text,
            parsed.statblock_json,
            parsed.file_path,
            parsed.mtime,
            parsed.hash,
        ],
    )?;
    // Refresh FTS for this row.
    conn.execute(
        "DELETE FROM entities_fts
         WHERE rowid = (SELECT rowid FROM entities WHERE id = ?1)",
        params![parsed.id],
    )?;
    conn.execute(
        "INSERT INTO entities_fts(rowid, title, body_text)
         SELECT rowid,
                COALESCE(json_extract(frontmatter, '$.title'), id),
                body_text
         FROM entities WHERE id = ?1",
        params![parsed.id],
    )?;
    tracing::debug!(id = parsed.id, path = parsed.file_path, "ingested");
    Ok(())
}

fn delete_by_path(path: &Path, vault_root: &Path, db: &Db) -> Result<()> {
    let rel = relative_path(path, vault_root);
    let conn = db.conn.lock().unwrap();
    conn.execute("DELETE FROM entities WHERE file_path = ?1", params![rel])?;
    Ok(())
}

fn parse_file(path: &Path, vault_root: &Path) -> Result<Parsed> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let (fm_yaml, body) = split_frontmatter(&raw);
    let fm: Frontmatter = if fm_yaml.trim().is_empty() {
        Frontmatter::default()
    } else {
        serde_yaml::from_str(fm_yaml).with_context(|| {
            format!("parsing YAML frontmatter in {}", path.display())
        })?
    };

    let rel = relative_path(path, vault_root);
    let id = fm
        .id
        .clone()
        .or_else(|| fm.title.clone())
        .map(|s| slugify(&s))
        .unwrap_or_else(|| slugify(&rel));

    let entity_type = fm.entity_type.clone().unwrap_or_else(|| "note".into());
    let campaign_id = fm.campaign_id.clone().unwrap_or_else(|| "_default".into());
    let license_provenance = fm
        .license_provenance
        .clone()
        .unwrap_or_else(|| "homebrew".into());
    let lens = fm.lens.clone();

    // Re-emit frontmatter as JSON for `entities.frontmatter`.
    let mut combined = serde_yaml::Mapping::new();
    if let Some(t) = fm.title.as_ref() {
        combined.insert("title".into(), t.clone().into());
    }
    if let Some(t) = fm.entity_type.as_ref() {
        combined.insert("type".into(), t.clone().into());
    }
    if let Some(l) = fm.lens.as_ref() {
        combined.insert("lens".into(), l.clone().into());
    }
    for (k, v) in fm.extras {
        combined.insert(k, v);
    }
    let frontmatter_value: Value = serde_yaml::from_value(serde_yaml::Value::Mapping(combined))
        .unwrap_or(Value::Object(Default::default()));
    let frontmatter_json = serde_json::to_string(&frontmatter_value)?;

    let body_text = strip_markdown(body);

    // Sidecar .statblock.json detection
    let statblock_json = sibling_statblock(path)?;

    let mtime = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let hash = simple_hash(&raw);

    Ok(Parsed {
        id,
        entity_type,
        campaign_id,
        lens,
        license_provenance,
        frontmatter_json,
        body: body.to_string(),
        body_text,
        statblock_json,
        file_path: rel,
        mtime,
        hash,
    })
}

fn sibling_statblock(md_path: &Path) -> Result<Option<String>> {
    let stem = md_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let parent = md_path.parent().unwrap_or(Path::new("."));
    let candidate = parent.join(format!("{stem}.statblock.json"));
    if !candidate.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&candidate)
        .with_context(|| format!("reading {}", candidate.display()))?;
    // Validate as JSON before storing
    let _: Value = serde_json::from_str(&raw)
        .with_context(|| format!("parsing JSON {}", candidate.display()))?;
    Ok(Some(raw))
}

fn split_frontmatter(s: &str) -> (&str, &str) {
    let trimmed = s.trim_start_matches('\u{FEFF}');
    let body_start_when_no_fm = || (s.len() - trimmed.len(), s);
    if !trimmed.starts_with(FRONTMATTER_DELIM) {
        let (_, b) = body_start_when_no_fm();
        return ("", b);
    }
    let after_first = &trimmed[FRONTMATTER_DELIM.len()..];
    let after_first = after_first.trim_start_matches(['\r', '\n']);
    if let Some(end) = after_first.find("\n---") {
        let yaml = &after_first[..end];
        let rest = &after_first[end + 4..];
        let body = rest.trim_start_matches(['\r', '\n']);
        (yaml, body)
    } else {
        ("", trimmed)
    }
}

fn strip_markdown(md: &str) -> String {
    use pulldown_cmark::{Event, Parser};
    let mut out = String::new();
    for event in Parser::new(md) {
        match event {
            Event::Text(t) | Event::Code(t) => {
                out.push_str(&t);
                out.push(' ');
            }
            Event::SoftBreak | Event::HardBreak | Event::End(_) => {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
            _ => {}
        }
    }
    out.trim().to_string()
}

fn relative_path(path: &Path, vault_root: &Path) -> String {
    path.strip_prefix(vault_root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn simple_hash(s: &str) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_frontmatter_basic() {
        let s = "---\ntitle: Foo\ntype: npc\n---\nbody here\nmore\n";
        let (fm, body) = split_frontmatter(s);
        assert!(fm.contains("title: Foo"));
        assert_eq!(body.trim(), "body here\nmore");
    }

    #[test]
    fn split_frontmatter_missing_returns_body_only() {
        let s = "no frontmatter here\n";
        let (fm, body) = split_frontmatter(s);
        assert_eq!(fm, "");
        assert!(body.contains("no frontmatter"));
    }

    #[test]
    fn slugify_collapses_punctuation() {
        assert_eq!(slugify("Lord Cassian!"), "lord-cassian");
        assert_eq!(slugify("YHWH (Lord of Hosts)"), "yhwh-lord-of-hosts");
    }

    #[test]
    fn strip_markdown_keeps_text_drops_syntax() {
        let md = "# Lord Cassian\n\n**Bold** and `code`. [link](http://x)\n";
        let plain = strip_markdown(md);
        assert!(plain.contains("Lord Cassian"));
        assert!(plain.contains("Bold"));
        assert!(plain.contains("code"));
        assert!(!plain.contains("**"));
    }

    #[test]
    fn session_artifacts_are_excluded_from_ingest_guards() {
        let artifact = Path::new("campaigns/c1/sessions/s1.log.md");
        assert!(is_session_artifact(artifact), "exported logs are session artifacts");
        // Guard composition: markdown + session artifact → skipped.
        assert!(is_markdown(artifact) && is_session_artifact(artifact));

        let note = Path::new("campaigns/c1/notes.md");
        assert!(!is_session_artifact(note), "regular vault markdown is not an artifact");
        assert!(is_markdown(note) && !is_session_artifact(note));

        // `.automerge` saves were never indexed (extension filter) — unchanged.
        let automerge = Path::new("campaigns/c1/sessions/s1.automerge");
        assert!(is_session_artifact(automerge));
        assert!(!is_markdown(automerge));
    }
}
