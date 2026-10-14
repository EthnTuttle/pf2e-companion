// Vitest unit tests for src/lib/wizard.svelte.ts
import { describe, expect, test, vi, beforeEach } from "vitest";
import {
  back,
  chooseLens,
  fetchWizardNeeded,
  next,
  resetWizard,
  shouldRedirectToWizard,
  validateTypedVaultPath,
  wizardProgress,
  wizardState,
} from "./wizard.svelte";
import { wizardStatus, wizardValidateVault } from "$lib/ipc";

vi.mock("$lib/ipc", () => ({
  wizardStatus: vi.fn(),
  wizardValidateVault: vi.fn(),
}));

beforeEach(() => {
  vi.clearAllMocks();
  resetWizard();
});

describe("shouldRedirectToWizard", () => {
  test("false when the wizard is not needed", () => {
    expect(shouldRedirectToWizard("/", false)).toBe(false);
    expect(shouldRedirectToWizard("/table", false)).toBe(false);
  });

  test("false inside the wizard (no redirect loops)", () => {
    expect(shouldRedirectToWizard("/onboarding", true)).toBe(false);
    expect(shouldRedirectToWizard("/onboarding/anything", true)).toBe(false);
  });

  test("true elsewhere when needed", () => {
    expect(shouldRedirectToWizard("/", true)).toBe(true);
    expect(shouldRedirectToWizard("/table", true)).toBe(true);
    expect(shouldRedirectToWizard("/vault/some-entity", true)).toBe(true);
  });
});

describe("fetchWizardNeeded", () => {
  test("true when wizard_status reports needed", async () => {
    vi.mocked(wizardStatus).mockResolvedValueOnce({
      needed: true,
      default_vault: "/home/u/vault",
    });
    expect(await fetchWizardNeeded()).toBe(true);
  });

  test("false on IPC failure — browser/LAN clients stay put (ac7)", async () => {
    vi.mocked(wizardStatus).mockRejectedValueOnce(new Error("no IPC"));
    expect(await fetchWizardNeeded()).toBe(false);
  });
});

describe("wizardProgress", () => {
  test("labels steps 0/1/2", () => {
    expect(wizardProgress(0)).toBe("Step 1 of 3 · Vault");
    expect(wizardProgress(1)).toBe("Step 2 of 3 · Lens");
    expect(wizardProgress(2)).toBe("Step 3 of 3 · LLM");
  });
});

describe("step machine", () => {
  test("next() advances 0→1→2 and clamps", () => {
    expect(wizardState.step).toBe(0);
    next();
    expect(wizardState.step).toBe(1);
    next();
    expect(wizardState.step).toBe(2);
    next();
    expect(wizardState.step).toBe(2);
  });

  test("back() reverses and clears the error", () => {
    wizardState.step = 2;
    wizardState.error = "boom";
    back();
    expect(wizardState.step).toBe(1);
    expect(wizardState.error).toBeNull();
    back();
    back();
    expect(wizardState.step).toBe(0);
  });

  test("chooseLens records the choice", () => {
    chooseLens("catholic");
    expect(wizardState.lens).toBe("catholic");
  });
});

describe("validateTypedVaultPath", () => {
  test("error set when validation fails (mock $lib/ipc)", async () => {
    vi.mocked(wizardValidateVault).mockRejectedValueOnce(
      "Vault path must be absolute",
    );
    const ok = await validateTypedVaultPath("relative/path");
    expect(ok).toBe(false);
    expect(wizardState.error).toContain("absolute");
    expect(wizardState.useDefaultVault).toBe(true);
  });

  test("normalized path stored on success", async () => {
    vi.mocked(wizardValidateVault).mockResolvedValueOnce("/home/gm/vault");
    const ok = await validateTypedVaultPath("/home/gm/vault");
    expect(ok).toBe(true);
    expect(wizardState.vaultPath).toBe("/home/gm/vault");
    expect(wizardState.useDefaultVault).toBe(false);
    expect(wizardState.error).toBeNull();
  });

  test("empty input is ignored without an IPC call", async () => {
    const ok = await validateTypedVaultPath("   ");
    expect(ok).toBe(false);
    expect(wizardValidateVault).not.toHaveBeenCalled();
    expect(wizardState.error).toBeNull();
  });
});
