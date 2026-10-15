// Lore-imagery client plumbing (docs/imagery/DEDUCTION.md §4.3).
//
// Everything degrades silently: a missing/unpromoted image resolves to null
// and callers render their shipped fallback. All resolutions are cached per
// path so seams never re-hit the IPC.

import { getEntity, getEntityImage } from "$lib/ipc";

const artCache = new Map<string, Promise<string | null>>();

/** Resolve a bundled art path (`<lens>/art/…​.webp`) to a data URL; null on miss. */
export function resolveArtUrl(
  packRelPath: string | null | undefined,
): Promise<string | null> {
  const p = (packRelPath ?? "").trim();
  if (!p) return Promise.resolve(null);
  let hit = artCache.get(p);
  if (!hit) {
    hit = getEntityImage(p).catch(() => null);
    artCache.set(p, hit);
  }
  return hit;
}

/** `lewisian:adventure-widows-lamp` → `lewisian`; null when shapeless. */
export function lensOfEntityId(id: string | null | undefined): string | null {
  if (!id) return null;
  const i = id.indexOf(":");
  return i > 0 ? id.slice(0, i) : null;
}

/** Convention: promoted adventure art lives at `<lens>/art/adventures/<slug>/<file>.webp`. */
export function adventureArt(lens: string, slug: string, file: string): string {
  return `${lens}/art/adventures/${slug}/${file}.webp`;
}

const SCENE_BG_RE = /^([a-z0-9-]+):adventure-([a-z0-9-]+)(?:#(scene-\d+))?$/;

/**
 * `scene.background_entity_id` → vista path (DEDUCTION §4.3.2). Accepts
 * `<lens>:adventure-<slug>` with an optional `#scene-N` fragment (default
 * scene-1). Any other value resolves to null — the grid keeps its
 * `.grid-bg.placeholder` fallback.
 */
export function sceneVistaPath(
  backgroundEntityId: string | null | undefined,
): string | null {
  if (!backgroundEntityId) return null;
  const m = SCENE_BG_RE.exec(backgroundEntityId.trim());
  if (!m) return null;
  return adventureArt(m[1], m[2], m[3] ?? "scene-1");
}


/** Frontmatter/handout `image` is pack-relative; prefix its lens. */
export function packArtPath(
  lens: string | null | undefined,
  image: string | null | undefined,
): string | null {
  const l = (lens ?? "").trim();
  const i = (image ?? "").trim();
  if (!l || !i) return null;
  return `${l}/${i}`;
}

export interface CoverInfo {
  url: string;
  alt: string;
}

const coverCache = new Map<string, Promise<CoverInfo | null>>();

/**
 * Adventure cover + the alt text shipped in its frontmatter
 * (`image` / `image_alt`). Null when the adventure has no promoted cover.
 */
export function resolveAdventureCover(
  lens: string,
  slug: string,
): Promise<CoverInfo | null> {
  const key = `${lens}:${slug}`;
  let hit = coverCache.get(key);
  if (!hit) {
    hit = (async () => {
      try {
        const ent = await getEntity(`${lens}:adventure-${slug}`);
        const image = ent?.frontmatter?.image;
        const alt = ent?.frontmatter?.image_alt;
        if (typeof image !== "string" || typeof alt !== "string" || !image || !alt) {
          return null;
        }
        const url = await resolveArtUrl(packArtPath(lens, image));
        return url ? { url, alt } : null;
      } catch {
        return null;
      }
    })();
    coverCache.set(key, hit);
  }
  return hit;
}
