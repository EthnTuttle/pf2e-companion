// Vitest unit tests for src/lib/diagnostics.ts
import { describe, expect, test, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { formatDiagnosticsMarkdown } from "./diagnostics";
import { diagnosticsGet } from "./ipc";
import type { DiagnosticsInfo } from "$lib/ipc";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

function info(overrides: Partial<DiagnosticsInfo> = {}): DiagnosticsInfo {
  return {
    app_version: "0.1.0",
    os: "linux",
    arch: "x86_64",
    session_id: "s1",
    lines: ["INFO ready", "WARN lag"],
    ...overrides,
  };
}

describe("formatDiagnosticsMarkdown", () => {
  test("includes version / OS / arch / session metadata lines", () => {
    const md = formatDiagnosticsMarkdown(info());
    expect(md).toContain("### pf2e-companion diagnostics");
    expect(md).toContain("- Version: 0.1.0");
    expect(md).toContain("- OS: linux");
    expect(md).toContain("- Arch: x86_64");
    expect(md).toContain("- Active session: s1");
  });

  test("wraps log lines in a details block, one line per entry", () => {
    const md = formatDiagnosticsMarkdown(info());
    expect(md).toContain("<details><summary>Last 2 log lines</summary>");
    expect(md).toContain("INFO ready\nWARN lag");
    expect(md).toContain("```");
    expect(md).toContain("</details>");
  });

  test("handles session_id null", () => {
    const md = formatDiagnosticsMarkdown(info({ session_id: null }));
    expect(md).toContain("- Active session: none");
  });

  test("handles empty lines", () => {
    const md = formatDiagnosticsMarkdown(info({ lines: [] }));
    expect(md).toContain("<details><summary>Last 0 log lines</summary>");
    expect(md).toContain("(no log lines captured)");
  });
});

describe("diagnosticsGet binding", () => {
  test("invokes diagnostics_get and passes the payload through verbatim", async () => {
    const payload: DiagnosticsInfo = info({ session_id: "abc" });
    vi.mocked(invoke).mockResolvedValueOnce(payload);
    const res = await diagnosticsGet();
    expect(invoke).toHaveBeenCalledWith("diagnostics_get");
    expect(res.session_id).toBe("abc");
    expect(res.lines).toEqual(payload.lines);
  });

  test("null session_id survives the round-trip (no active session)", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(info({ session_id: null }));
    const res = await diagnosticsGet();
    expect(res.session_id).toBeNull();
  });
});
