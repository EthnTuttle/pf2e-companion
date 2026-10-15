// Saved-session helpers + the shared export-session-log flow.
// Used by both the /table lobby list and the play surface GM drawer.
import { sessionExportLog, type SavedSessionSummary } from "$lib/ipc";

/** Default save-as filename: `<slug|id-prefix>-YYYY-MM-DD.log.md` (UTC). */
export function defaultLogFilename(name: string | null, id: string, dateMs: number): string {
  const slug = (name ?? "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  const base = slug || id.slice(0, 8);
  const date = new Date(dateMs).toISOString().slice(0, 10);
  return `${base}-${date}.log.md`;
}

/** One-line lobby description: `name — campaign • adventure • date`. */
export function describeSavedSession(s: SavedSessionSummary): string {
  const adventure = s.adventure_slug ?? "no adventure";
  const date = new Date(s.created_at).toISOString().slice(0, 10);
  return `${s.name} — ${s.campaign} • ${adventure} • ${date}`;
}

/**
 * Export a session log through a save-as dialog. Shared by the lobby and
 * the GM drawer.
 *
 * - Dialog cancelled → `null`, nothing written.
 * - Dialog unavailable (unsupported platform / plugin missing) → fall
 *   back to `sessionExportLog(id, null)` (vault default location) and
 *   return that path.
 * - Otherwise export to the chosen destination and return the path.
 */
export async function exportSessionLogViaDialog(s: {
  name: string | null;
  session_id: string;
}): Promise<string | null> {
  let dest: string | null;
  try {
    const { save } = await import("@tauri-apps/plugin-dialog");
    dest = await save({
      defaultPath: defaultLogFilename(s.name, s.session_id, Date.now()),
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });
  } catch {
    return await sessionExportLog(s.session_id, null);
  }
  if (dest === null) return null;
  return await sessionExportLog(s.session_id, dest);
}
