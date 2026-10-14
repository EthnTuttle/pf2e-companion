//! Adventure pack parsing + validation (CONTRACT § 5).
//!
//! Adventure entities live at `data/content/lewisian/adventures/` as a
//! markdown file with `scenes` / `handouts` / `npcs` frontmatter keys
//! (preserved into `entities.frontmatter` by the content loader's
//! catch-all extras mapping) plus `<adventure>.<npc>.statblock.json`
//! sidecars. The full parsed def is re-serialized into the entity's
//! `statblock` column so the `load_adventure` IPC path can re-read it
//! without touching the filesystem.

use crate::db::Db;
use anyhow::{anyhow, bail, Context, Result};
use include_dir::Dir;
use serde::{Deserialize, Serialize};
use serde_yaml::Value as Yaml;

/// An encounter block on a scene.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Encounter {
    pub difficulty: String,
    #[serde(default)]
    pub npcs: Vec<String>,
}

/// One scene of an adventure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Scene {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub read_aloud: Option<String>,
    #[serde(default)]
    pub encounter: Option<Encounter>,
}

/// A player handout.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HandoutDef {
    pub id: String,
    #[serde(default)]
    pub title: String,
    /// `"all"` or `"gm"`.
    #[serde(default)]
    pub visible_to: String,
    #[serde(default)]
    pub body: Option<String>,
}

/// An NPC sidecar reference (slug + parsed statblock JSON).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NpcRef {
    pub slug: String,
    pub statblock: serde_json::Value,
}

/// The parsed adventure definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdventureDef {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub scenes: Vec<Scene>,
    #[serde(default)]
    pub handouts: Vec<HandoutDef>,
    #[serde(default)]
    pub npcs: Vec<NpcRef>,
    /// Encounter-roster NPCs resolved from their own sidecars (slug →
    /// statblock). Additive (serde default): distinct from the top-level
    /// `npcs` list so the contract's "3 NPCs" count is unaffected.
    #[serde(default)]
    pub roster: std::collections::BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub body: String,
}

/// Parse an adventure from its frontmatter mapping + body, loading NPC
/// sidecars from the adventure's own directory `dir`, anchored by the
/// adventure stem (`<stem>.<slug>.statblock.json` — CONTRACT § 5).
pub fn parse_adventure(
    fm: &serde_yaml::Mapping,
    body: &str,
    dir: &Dir<'_>,
    stem: &str,
) -> Result<AdventureDef> {
    let get = |key: &str| fm.get(serde_yaml::Value::from(key)).cloned();

    let id = get("id")
        .and_then(|v| v.as_str().map(str::to_string))
        .ok_or_else(|| anyhow!("adventure frontmatter missing `id`"))?;
    let title = get("title")
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| id.clone());

    let mut scenes = Vec::new();
    if let Some(Yaml::Sequence(seq)) = get("scenes") {
        for item in seq {
            let Yaml::Mapping(m) = item else { continue };
            let pick = |k: &str| m.get(Yaml::from(k)).cloned();
            let scene_id = pick("id")
                .and_then(|v| v.as_str().map(str::to_string))
                .ok_or_else(|| anyhow!("scene missing `id` in adventure {id}"))?;
            let encounter = pick("encounter").and_then(|v| {
                let Yaml::Mapping(e) = v else { return None };
                let difficulty = e
                    .get(Yaml::from("difficulty"))
                    .and_then(|d| d.as_str().map(str::to_string))
                    .unwrap_or_else(|| "moderate".into());
                let npcs = match e.get(Yaml::from("npcs")) {
                    Some(Yaml::Sequence(list)) => list
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect(),
                    _ => Vec::new(),
                };
                Some(Encounter { difficulty, npcs })
            });
            scenes.push(Scene {
                id: scene_id,
                title: pick("title")
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default(),
                read_aloud: pick("read_aloud")
                    .and_then(|v| v.as_str().map(str::to_string)),
                encounter,
            });
        }
    }

    let mut handouts = Vec::new();
    if let Some(Yaml::Sequence(seq)) = get("handouts") {
        for item in seq {
            let Yaml::Mapping(m) = item else { continue };
            let pick = |k: &str| m.get(Yaml::from(k)).cloned();
            let handout_id = pick("id")
                .and_then(|v| v.as_str().map(str::to_string))
                .ok_or_else(|| anyhow!("handout missing `id` in adventure {id}"))?;
            handouts.push(HandoutDef {
                id: handout_id,
                title: pick("title")
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default(),
                visible_to: pick("visible_to")
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_else(|| "gm".into()),
                body: pick("body").and_then(|v| v.as_str().map(str::to_string)),
            });
        }
    }

    let mut npcs = Vec::new();
    if let Some(Yaml::Sequence(seq)) = get("npcs") {
        for item in seq {
            let Some(slug) = item.as_str().map(str::to_string) else { continue };
            let statblock = load_sidecar(dir, stem, &slug, &id)
                .with_context(|| format!("adventure {id}: npc sidecar `{slug}`"))?;
            npcs.push(NpcRef { slug, statblock });
        }
    }

    // Encounter-roster slugs resolve from their own sidecars best-effort
    // (a missing one is a validation problem, not a parse failure —
    // startup logs-and-skips, never panics).
    let mut roster = std::collections::BTreeMap::new();
    for scene in &scenes {
        let Some(encounter) = &scene.encounter else { continue };
        for slug in &encounter.npcs {
            if npcs.iter().any(|n| &n.slug == slug) || roster.contains_key(slug) {
                continue;
            }
            if let Ok(statblock) = load_sidecar(dir, stem, slug, &id) {
                roster.insert(slug.clone(), statblock);
            }
        }
    }

    Ok(AdventureDef { id, title, scenes, handouts, npcs, roster, body: body.to_string() })
}

/// Load `<stem>.<slug>.statblock.json` from the adventure's own
/// directory (exact filename, CONTRACT § 5 — never a pack-wide glob).
/// NB: include_dir entry paths are root-relative, so `get_file` is not
/// usable from a sub-`Dir`; direct children are matched by file name.
fn load_sidecar(dir: &Dir<'_>, stem: &str, slug: &str, adventure_id: &str) -> Result<serde_json::Value> {
    let name = format!("{stem}.{slug}.statblock.json");
    let file = dir
        .files()
        .find(|f| {
            f.path().file_name().and_then(|n| n.to_str()) == Some(name.as_str())
        })
        .ok_or_else(|| {
            anyhow!("no `{name}` sidecar for npc `{slug}` (adventure {adventure_id})")
        })?;
    let raw = std::str::from_utf8(file.contents())
        .with_context(|| format!("non-utf8 sidecar {}", file.path().display()))?;
    serde_json::from_str(raw).with_context(|| format!("parse {}", file.path().display()))
}

/// Structural validation. Returns a list of problems (empty = valid).
/// Includes `rules::validate_statblock` per NPC sidecar.
pub fn validate_adventure(def: &AdventureDef) -> Vec<String> {
    let mut problems = Vec::new();

    let mut seen = std::collections::BTreeSet::new();
    for scene in &def.scenes {
        if !seen.insert(scene.id.clone()) {
            problems.push(format!("duplicate scene id `{}`", scene.id));
        }
        if let Some(enc) = &scene.encounter {
            if !["trivial", "low", "moderate", "severe", "extreme"]
                .contains(&enc.difficulty.as_str())
            {
                problems.push(format!(
                    "scene `{}` encounter difficulty `{}` is not a PF2e difficulty",
                    scene.id, enc.difficulty
                ));
            }
            // Every rostered slug must have a sidecar — a roster entry
            // without one is silently un-playable, so it FAILS validation.
            for slug in &enc.npcs {
                let resolvable =
                    def.npcs.iter().any(|n| &n.slug == slug) || def.roster.contains_key(slug);
                if !resolvable {
                    problems.push(format!(
                        "scene `{}` rosters npc `{slug}` without a sidecar",
                        scene.id
                    ));
                }
            }
        }
    }

    let mut seen = std::collections::BTreeSet::new();
    for handout in &def.handouts {
        if !seen.insert(handout.id.clone()) {
            problems.push(format!("duplicate handout id `{}`", handout.id));
        }
        if !["all", "gm"].contains(&handout.visible_to.as_str()) {
            problems.push(format!(
                "handout `{}` visible_to `{}` not in [all, gm]",
                handout.id, handout.visible_to
            ));
        }
    }

    for npc in &def.npcs {
        let result = crate::rules::validate_statblock(&npc.statblock);
        if !result.valid {
            problems.push(format!(
                "npc `{}` statblock invalid: {}",
                npc.slug,
                result.errors.join("; ")
            ));
        }
    }

    problems
}

/// Resolve an adventure def by slug from the indexed entities (the
/// `load_adventure` IPC path). Prefers the serialized def in the
/// `statblock` column; falls back to re-parsing the frontmatter.
pub fn load_adventure_def(db: &Db, slug: &str) -> Result<AdventureDef> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, frontmatter, statblock FROM entities WHERE type = 'adventure'",
    )?;
    let rows: Vec<(String, String, Option<String>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    drop(stmt);
    drop(conn);

    for (id, frontmatter, statblock) in rows {
        let id_tail = id.rsplit([':', '/']).next().unwrap_or(&id).to_string();
        let matches_slug = id_tail == slug
            || id_tail == format!("adventure-{slug}")
            || id == slug;
        if !matches_slug {
            continue;
        }
        if let Some(sb) = statblock {
            if let Ok(def) = serde_json::from_str::<AdventureDef>(&sb) {
                return Ok(def);
            }
        }
        // Fall back to the frontmatter columns (no sidecars available
        // at runtime — they only exist at compile time).
        let fm_value: serde_json::Value = serde_json::from_str(&frontmatter)?;
        let yaml_value: Yaml =
            serde_yaml::from_str(&serde_json::to_string(&fm_value)?)?;
        if let Yaml::Mapping(m) = yaml_value {
            let mut def = def_from_mapping(&m, &id)?;
            def.npcs = def
                .npcs
                .into_iter()
                .map(|n| NpcRef {
                    slug: n.slug,
                    statblock: serde_json::Value::Null,
                })
                .collect();
            return Ok(def);
        }
        bail!("adventure `{id}` frontmatter is not a mapping");
    }
    bail!("no bundled adventure matches slug `{slug}`")
}

/// Frontmatter-only parse (no sidecars — statblocks come back null).
fn def_from_mapping(fm: &serde_yaml::Mapping, fallback_id: &str) -> Result<AdventureDef> {
    let get = |key: &str| fm.get(serde_yaml::Value::from(key)).cloned();
    let id = get("id")
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| fallback_id.to_string());
    let title = get("title")
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| id.clone());
    let mut scenes = Vec::new();
    if let Some(Yaml::Sequence(seq)) = get("scenes") {
        for item in seq {
            let Yaml::Mapping(m) = item else { continue };
            let pick = |k: &str| m.get(Yaml::from(k)).cloned();
            scenes.push(Scene {
                id: pick("id")
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default(),
                title: pick("title")
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default(),
                read_aloud: pick("read_aloud")
                    .and_then(|v| v.as_str().map(str::to_string)),
                encounter: pick("encounter").and_then(|v| {
                    let Yaml::Mapping(e) = v else { return None };
                    Some(Encounter {
                        difficulty: e
                            .get(Yaml::from("difficulty"))
                            .and_then(|d| d.as_str().map(str::to_string))
                            .unwrap_or_else(|| "moderate".into()),
                        npcs: match e.get(Yaml::from("npcs")) {
                            Some(Yaml::Sequence(list)) => list
                                .iter()
                                .filter_map(|v| v.as_str().map(str::to_string))
                                .collect(),
                            _ => Vec::new(),
                        },
                    })
                }),
            });
        }
    }
    let mut handouts = Vec::new();
    if let Some(Yaml::Sequence(seq)) = get("handouts") {
        for item in seq {
            let Yaml::Mapping(m) = item else { continue };
            let pick = |k: &str| m.get(Yaml::from(k)).cloned();
            handouts.push(HandoutDef {
                id: pick("id")
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default(),
                title: pick("title")
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default(),
                visible_to: pick("visible_to")
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_else(|| "gm".into()),
                body: pick("body").and_then(|v| v.as_str().map(str::to_string)),
            });
        }
    }
    let npcs = match get("npcs") {
        Some(Yaml::Sequence(list)) => list
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .map(|slug| NpcRef { slug, statblock: serde_json::Value::Null })
            .collect(),
        _ => Vec::new(),
    };
    Ok(AdventureDef { id, title, scenes, handouts, npcs, roster: Default::default(), body: String::new() })
}

