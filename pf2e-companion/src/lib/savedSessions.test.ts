// Vitest unit tests for src/lib/savedSessions.svelte.ts
import { describe, expect, test, vi, beforeEach } from "vitest";
import { defaultLogFilename, describeSavedSession, exportSessionLogViaDialog } from "./savedSessions.svelte";
import { sessionExportLog, type SavedSessionSummary } from "$lib/ipc";
import { save } from "@tauri-apps/plugin-dialog";

vi.mock("$lib/ipc", () => ({
  sessionExportLog: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: vi.fn(),
}));

const oct14 = Date.UTC(2026, 9, 14);

function summary(overrides: Partial<SavedSessionSummary> = {}): SavedSessionSummary {
  return {
    session_id: "s1",
    campaign: "friday",
    name: "Friday Table",
    created_at: oct14,
    adventure_slug: null,
    ended: false,
    saved_at: oct14,
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("defaultLogFilename", () => {
  test("slugifies the session name and appends the UTC date", () => {
    expect(defaultLogFilename("Friday Table!", "s1", oct14)).toBe(
      "friday-table-2026-10-14.log.md",
    );
  });

  test("falls back to the id prefix when the name is empty or null", () => {
    expect(defaultLogFilename("", "12345678-abcd", oct14)).toBe(
      "12345678-2026-10-14.log.md",
    );
    expect(defaultLogFilename(null, "12345678-abcd", oct14)).toBe(
      "12345678-2026-10-14.log.md",
    );
  });

  test("uses UTC dates at the boundary", () => {
    // One ms before UTC midnight of Oct 14 → still Oct 13 in UTC.
    expect(defaultLogFilename("T", "s1", oct14 - 1)).toBe("t-2026-10-13.log.md");
    expect(defaultLogFilename("T", "s1", oct14)).toBe("t-2026-10-14.log.md");
  });
});

describe("describeSavedSession", () => {
  test("includes name, campaign, adventure and date", () => {
    const s = summary({ adventure_slug: "widows-lamp" });
    expect(describeSavedSession(s)).toBe(
      "Friday Table — friday • widows-lamp • 2026-10-14",
    );
  });

  test("says 'no adventure' when the adventure is null", () => {
    expect(describeSavedSession(summary())).toBe(
      "Friday Table — friday • no adventure • 2026-10-14",
    );
  });
});

describe("exportSessionLogViaDialog", () => {
  test("exports to the chosen destination", async () => {
    vi.mocked(save).mockResolvedValue("/home/gm/logs/my-log.md");
    vi.mocked(sessionExportLog).mockResolvedValue("/home/gm/logs/my-log.md");

    const path = await exportSessionLogViaDialog({ name: "Friday Table", session_id: "s1" });

    expect(path).toBe("/home/gm/logs/my-log.md");
    expect(save).toHaveBeenCalledTimes(1);
    expect(save).toHaveBeenCalledWith(
      expect.objectContaining({
        defaultPath: expect.stringMatching(/^friday-table-\d{4}-\d{2}-\d{2}\.log\.md$/),
        filters: [{ name: "Markdown", extensions: ["md"] }],
      }),
    );
    expect(sessionExportLog).toHaveBeenCalledWith("s1", "/home/gm/logs/my-log.md");
  });

  test("cancel writes nothing and returns null", async () => {
    vi.mocked(save).mockResolvedValue(null);

    const path = await exportSessionLogViaDialog({ name: "Friday Table", session_id: "s1" });

    expect(path).toBeNull();
    expect(sessionExportLog).not.toHaveBeenCalled();
  });

  test("dialog error falls back to the vault default location", async () => {
    vi.mocked(save).mockRejectedValue(new Error("unsupported platform"));
    vi.mocked(sessionExportLog).mockResolvedValue("/vault/campaigns/friday/sessions/s1.log.md");

    const path = await exportSessionLogViaDialog({ name: "Friday Table", session_id: "s1" });

    expect(path).toBe("/vault/campaigns/friday/sessions/s1.log.md");
    expect(sessionExportLog).toHaveBeenCalledWith("s1", null);
  });
});
