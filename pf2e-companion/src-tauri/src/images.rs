//! Bundled lore-image serving (imagery v1 — docs/imagery/DEDUCTION.md §4.3).
//!
//! Promoted art lives under `data/content/<lens>/art/**` and rides the same
//! `include_dir!` embed as the markdown packs, so it needs zero loader
//! changes. One funnel — [`entity_image_data_url`] — resolves a pack-relative
//! path to a `data:` URL for the single `get_entity_image` IPC. Misses are
//! ordinary errors: art never blocks code, and the frontend swaps in its
//! shipped fallback whenever a path does not resolve.

use anyhow::{bail, Context, Result};
use include_dir::{include_dir, Dir};

static CONTENT_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/data/content");

/// Resolve `"<lens>/art/…​.webp"` to `data:image/webp;base64,…`.
///
/// `pack_rel_path` is the pack-relative image path joined to its lens id
/// (e.g. `lewisian/art/adventures/widows-lamp/cover.webp`).
pub fn entity_image_data_url(pack_rel_path: &str) -> Result<String> {
    serve(&CONTENT_DIR, pack_rel_path)
}

/// Existence probe for token stamping (imagery v1, DEDUCTION §4.3): `true`
/// only when a sanitized `.webp`/`.png` path resolves to a bundled file.
/// Same embed as [`entity_image_data_url`] — never touches disk/network.
pub fn bundled_image_exists(pack_rel_path: &str) -> bool {
    match sanitize(pack_rel_path) {
        Ok(rel) => mime_for(rel).is_ok() && CONTENT_DIR.get_file(rel).is_some(),
        Err(_) => false,
    }
}

/// Root-injectable core (integration tests drive it against a fixture tree).
pub(crate) fn serve(root: &Dir<'_>, pack_rel_path: &str) -> Result<String> {
    let rel = sanitize(pack_rel_path)?;
    let mime = mime_for(rel)?;
    let file = root
        .get_file(rel)
        .with_context(|| format!("no bundled image at `{rel}`"))?;
    Ok(format!("data:{mime};base64,{}", base64_encode(file.contents())))
}

/// Traversal guard: relative, forward-slash-separated, no `..`, no empties.
fn sanitize(rel: &str) -> Result<&str> {
    let rel = rel.trim();
    if rel.is_empty() {
        bail!("empty image path");
    }
    if rel.starts_with('/') || rel.contains('\\') {
        bail!("image path must be relative with forward slashes: `{rel}`");
    }
    if rel.split('/').any(|seg| seg.is_empty() || seg == "..") {
        bail!("image path has an empty/traversal segment: `{rel}`");
    }
    Ok(rel)
}

fn mime_for(rel: &str) -> Result<&'static str> {
    if rel.ends_with(".webp") {
        Ok("image/webp")
    } else if rel.ends_with(".png") {
        Ok("image/png")
    } else {
        bail!("unsupported image extension in `{rel}` (webp/png only)")
    }
}

/// RFC 4648 base64. std-only: the crate tree gains no dependencies.
fn base64_encode(bytes: &[u8]) -> String {
    const TBL: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = b0 << 16 | b1 << 8 | b2;
        out.push(TBL[(n >> 18) as usize & 63] as char);
        out.push(TBL[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TBL[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TBL[n as usize & 63] as char } else { '=' });
    }
    out
}
