//! Content-pack loader.
//!
//! Bundled lens packs live under `data/content/<lens>/` and are embedded into
//! the binary via `include_dir!`. At startup, [`load_bundled_packs`] iterates
//! every embedded markdown file and inserts a row into `entities` tagged
//! `source = 'reference'`. Sidecar `<stem>.statblock.json` files attach as the
//! row's statblock column.
//!
//! The loader is independent of the runtime [`vault`] watcher: bundled content
//! is read-only and lives in app-state (not on the filesystem); user-authored
//! content lives in the vault and goes through `vault::ingest_file`.

use crate::db::Db;
use anyhow::{Context, Result};
use include_dir::{include_dir, Dir, DirEntry};
use rusqlite::params;
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;

const FRONTMATTER_DELIM: &str = "---";

static CONTENT_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/data/content");

#[derive(Default, Deserialize)]
struct BundledFrontmatter {
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
    #[serde(flatten)]
    extras: serde_yaml::Mapping,
}

pub struct LoadReport {
    pub packs_loaded: usize,
    pub entities_loaded: usize,
}

/// Walk the bundled `data/content/` tree and ingest every `.md` file.
/// Idempotent — uses `INSERT OR REPLACE` on `entities` keyed by id.
pub fn load_bundled_packs(db: &Db) -> Result<LoadReport> {
    let mut report = LoadReport {
        packs_loaded: 0,
        entities_loaded: 0,
    };

    for pack_dir in CONTENT_DIR.dirs() {
        let lens_id = pack_dir
            .path()
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if lens_id.is_empty() {
            continue;
        }
        report.packs_loaded += 1;

        let mut conn = db.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut upsert = tx.prepare(
                r#"
                INSERT INTO entities
                  (id, type, campaign_id, source, lens, license_provenance,
                   frontmatter, body, body_text, statblock, file_path, mtime, hash)
                VALUES (?1, ?2, '_reference', 'reference', ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10)
                ON CONFLICT(id) DO UPDATE SET
                  type = excluded.type,
                  lens = excluded.lens,
                  license_provenance = excluded.license_provenance,
                  frontmatter = excluded.frontmatter,
                  body = excluded.body,
                  body_text = excluded.body_text,
                  statblock = excluded.statblock,
                  file_path = excluded.file_path,
                  hash = excluded.hash
                "#,
            )?;
            let mut refresh_fts = tx.prepare(
                "DELETE FROM entities_fts
                 WHERE rowid = (SELECT rowid FROM entities WHERE id = ?1)",
            )?;
            let mut insert_fts = tx.prepare(
                "INSERT INTO entities_fts(rowid, title, body_text)
                 SELECT rowid,
                        COALESCE(json_extract(frontmatter, '$.title'), id),
                        body_text
                 FROM entities WHERE id = ?1",
            )?;

            walk_dir(pack_dir, &mut |entry| -> Result<()> {
                let path = entry.path();
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                if !name.ends_with(".md") {
                    return Ok(());
                }
                let raw = std::str::from_utf8(entry.contents())
                    .with_context(|| format!("non-utf8 content in {}", path.display()))?;
                let parsed = parse_one(path, raw, lens_id, pack_dir)?;
                upsert.execute(params![
                    parsed.id,
                    parsed.entity_type,
                    parsed.lens,
                    parsed.license_provenance,
                    parsed.frontmatter_json,
                    parsed.body,
                    parsed.body_text,
                    parsed.statblock_json,
                    parsed.file_path,
                    parsed.hash,
                ])?;
                refresh_fts.execute(params![parsed.id])?;
                insert_fts.execute(params![parsed.id])?;
                report.entities_loaded += 1;
                Ok(())
            })?;
        }
        tx.commit()?;
    }

    tracing::info!(
        packs = report.packs_loaded,
        entities = report.entities_loaded,
        "loaded bundled content packs"
    );
    Ok(report)
}

struct ParsedBundled {
    id: String,
    entity_type: String,
    lens: String,
    license_provenance: String,
    frontmatter_json: String,
    body: String,
    body_text: String,
    statblock_json: Option<String>,
    file_path: String,
    hash: String,
}

/// Adventure def attach for `type: adventure` entities (real build).
#[cfg(not(test))]
fn adventure_statblock(
    combined: &serde_yaml::Mapping,
    body: &str,
    pack_dir: &Dir<'_>,
    adventure_dir: Option<&Dir<'_>>,
    stem: &str,
    id: &str,
    fallback: Option<String>,
) -> Option<String> {
    // Sidecars resolve within the adventure's own dir, anchored by the
    // adventure stem (CONTRACT § 5); fall back to the pack dir when the
    // entity somehow has no directory of its own.
    let dir = adventure_dir.unwrap_or(pack_dir);
    match crate::adventure::parse_adventure(combined, body, dir, stem) {
        Ok(def) => {
            let problems = crate::adventure::validate_adventure(&def);
            if problems.is_empty() {
                tracing::debug!(id, "bundled adventure parsed");
            } else {
                tracing::warn!(
                    id,
                    problems = problems.join("; "),
                    "bundled adventure validation issues"
                );
            }
            serde_json::to_string(&def).ok()
        }
        Err(e) => {
            tracing::warn!(id, error = %e, "failed to parse bundled adventure; skipping def");
            fallback
        }
    }
}

/// Standalone (#[path]-included) compilations skip the adventure hook.
#[cfg(test)]
fn adventure_statblock(
    _combined: &serde_yaml::Mapping,
    _body: &str,
    _pack_dir: &Dir<'_>,
    _adventure_dir: Option<&Dir<'_>>,
    _stem: &str,
    _id: &str,
    fallback: Option<String>,
) -> Option<String> {
    fallback
}

fn parse_one(
    path: &Path,
    raw: &str,
    lens_id: &str,
    pack_dir: &Dir<'_>,
) -> Result<ParsedBundled> {
    let (fm_yaml, body) = split_frontmatter(raw);
    let fm: BundledFrontmatter = if fm_yaml.trim().is_empty() {
        BundledFrontmatter::default()
    } else {
        serde_yaml::from_str(fm_yaml)
            .with_context(|| format!("parsing frontmatter in {}", path.display()))?
    };

    let id = fm.id.clone().unwrap_or_else(|| {
        format!(
            "{}:{}",
            lens_id,
            slug(
                fm.title
                    .as_deref()
                    .unwrap_or_else(|| path.file_stem().and_then(|s| s.to_str()).unwrap_or("entry"))
            )
        )
    });
    let entity_type = fm.entity_type.clone().unwrap_or_else(|| "note".into());
    let lens = fm.lens.clone().unwrap_or_else(|| lens_id.to_string());
    let license_provenance = fm
        .license_provenance
        .clone()
        .unwrap_or_else(|| "homebrew".into());

    // Re-emit frontmatter as JSON for searchability.
    let mut combined = serde_yaml::Mapping::new();
    // `id` is needed downstream (adventure parsing looks it up in the
    // mapping) and is useful in the searchable frontmatter JSON too.
    combined.insert("id".into(), id.clone().into());
    if let Some(t) = fm.title.as_ref() {
        combined.insert("title".into(), t.clone().into());
    }
    if let Some(t) = fm.entity_type.as_ref() {
        combined.insert("type".into(), t.clone().into());
    }
    combined.insert("lens".into(), lens.clone().into());
    for (k, v) in fm.extras {
        combined.insert(k, v);
    }
    let fm_value: Value =
        serde_yaml::from_value(serde_yaml::Value::Mapping(combined.clone()))
            .unwrap_or(Value::Null);
    let frontmatter_json = serde_json::to_string(&fm_value)?;

    let body_text = strip_markdown(body);

    // Sidecar statblock JSON in the same directory, with the same stem.
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let parent = path.parent().map(Path::to_path_buf);
    let statblock_path = parent
        .as_deref()
        .map(|p| p.join(format!("{stem}.statblock.json")));
    let statblock_json = statblock_path
        .as_deref()
        .and_then(|sp| pack_dir.get_file(sp))
        .map(|f| String::from_utf8_lossy(f.contents()).into_owned());

    // Phase 10 — adventure entities: parse the full def (scenes /
    // handouts / npc sidecars, CONTRACT § 5) and store it in the
    // statblock column so the `load_adventure` IPC path can re-read it.
    // Log-and-skip on failure — bundled content never panics startup.
    // (Legacy phase tests compile this module standalone via #[path];
    // the hook needs the full crate tree and is cfg'd out there.)
    let statblock_json = if entity_type == "adventure" {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let adventure_dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .and_then(|p| pack_dir.get_dir(p));
        adventure_statblock(&combined, body, pack_dir, adventure_dir, stem, &id, statblock_json)
    } else {
        statblock_json
    };

    let file_path = format!("bundled:{}", path.display());
    let hash = simple_hash(raw);

    Ok(ParsedBundled {
        id,
        entity_type,
        lens,
        license_provenance,
        frontmatter_json,
        body: body.to_string(),
        body_text,
        statblock_json,
        file_path,
        hash,
    })
}

fn walk_dir<'a>(
    dir: &Dir<'a>,
    on_file: &mut dyn FnMut(&include_dir::File<'a>) -> Result<()>,
) -> Result<()> {
    for entry in dir.entries() {
        match entry {
            DirEntry::Dir(d) => walk_dir(d, on_file)?,
            DirEntry::File(f) => on_file(f)?,
        }
    }
    Ok(())
}

fn split_frontmatter(s: &str) -> (&str, &str) {
    let trimmed = s.trim_start_matches('\u{FEFF}');
    if !trimmed.starts_with(FRONTMATTER_DELIM) {
        return ("", trimmed);
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

fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut prev = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev = false;
        } else if !prev && !out.is_empty() {
            out.push('-');
            prev = true;
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
        let (fm, body) = split_frontmatter("---\ntitle: A\n---\nbody\n");
        assert!(fm.contains("title: A"));
        assert!(body.starts_with("body"));
    }

    #[test]
    fn slug_normalizes() {
        assert_eq!(slug("Heaven (the Throne Room)"), "heaven-the-throne-room");
    }
}
