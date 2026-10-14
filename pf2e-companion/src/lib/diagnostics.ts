// Diagnostics markdown formatter. Pure functions; no Svelte / Tauri / DOM
// deps (mirrors citation.ts). Turns the `diagnostics_get` blob into a
// paste-ready GitHub comment.

import type { DiagnosticsInfo } from "./ipc";

/**
 * Render the diagnostics blob as a GitHub-flavored markdown snippet:
 * a metadata header (version / OS / arch / active session) followed by
 * the last N log lines inside a collapsed `<details>` block.
 */
export function formatDiagnosticsMarkdown(info: DiagnosticsInfo): string {
  const session = info.session_id ?? "none";
  const lines = info.lines.length
    ? info.lines.join("\n")
    : "(no log lines captured)";
  return [
    "### pf2e-companion diagnostics",
    "",
    `- Version: ${info.app_version}`,
    `- OS: ${info.os}`,
    `- Arch: ${info.arch}`,
    `- Active session: ${session}`,
    "",
    `<details><summary>Last ${info.lines.length} log lines</summary>`,
    "",
    "```",
    lines,
    "```",
    "",
    "</details>",
  ].join("\n");
}
