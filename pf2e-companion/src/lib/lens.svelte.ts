// Shared lens state. SvelteKit + Svelte 5 runes; lives across routes.
import { listLenses, type LensManifest } from "$lib/ipc";

const LENS_KEY = "pf2e.lens";

const storedLens =
  typeof localStorage !== "undefined" ? localStorage.getItem(LENS_KEY) : null;

export const lensState = $state({
  active: storedLens ?? "lewisian",
  manifests: [] as LensManifest[],
  loaded: false,
});

/** Set the active lens and persist it (`pf2e.lens`, same convention as
 * `pf2e.playerName`). */
export function setActiveLens(id: string) {
  lensState.active = id;
  try {
    localStorage.setItem(LENS_KEY, id);
  } catch {
    // Storage unavailable (private mode etc.) — in-memory only.
  }
}

export async function ensureLensesLoaded() {
  if (lensState.loaded) return;
  try {
    lensState.manifests = await listLenses();
  } catch {
    lensState.manifests = [];
  }
  lensState.loaded = true;
}
