// First-run wizard state. Svelte 5 runes; mirrors session.svelte.ts shape.
import { wizardStatus, wizardValidateVault } from "$lib/ipc";
import { setActiveLens } from "$lib/lens.svelte";

export const wizardState = $state({
  step: 0,
  vaultPath: null as string | null,
  useDefaultVault: true,
  lens: "lewisian",
  llmChoice: "skip" as "skip" | "ollama" | "anthropic",
  error: null as string | null,
});

/** Pure: should the app route away to /onboarding? Never true once we're
 * already inside the wizard (prevents redirect loops). */
export function shouldRedirectToWizard(pathname: string, needed: boolean): boolean {
  return needed && !pathname.startsWith("/onboarding");
}

/** First-run gate for the layout onMount: `true` only when wizard_status
 * says onboarding is needed. IPC failure (browser/LAN clients have no
 * Tauri internals) resolves false — the visitor just stays put. */
export async function fetchWizardNeeded(): Promise<boolean> {
  try {
    return (await wizardStatus()).needed;
  } catch {
    return false;
  }
}

/** Pure: "Step 1 of 3 · Vault" style header. Out-of-range steps clamp. */
export function wizardProgress(step: number): string {
  const clamped = Math.min(Math.max(step, 0), 2);
  const names = ["Vault", "Lens", "LLM"];
  return `Step ${clamped + 1} of 3 · ${names[clamped]}`;
}

/** Advance to the next screen (0→1→2). */
export function next() {
  if (wizardState.step < 2) wizardState.step += 1;
}

/** Back to the previous screen. */
export function back() {
  wizardState.error = null;
  if (wizardState.step > 0) wizardState.step -= 1;
}

/** A typed vault path (screen 1 optional input): validate via IPC, keep
 * the normalized path on success. */
export async function validateTypedVaultPath(raw: string) {
  wizardState.error = null;
  const trimmed = raw.trim();
  if (!trimmed) return false;
  try {
    wizardState.vaultPath = await wizardValidateVault(trimmed);
    wizardState.useDefaultVault = false;
    return true;
  } catch (e) {
    wizardState.error = String(e);
    return false;
  }
}

/** Record the lens choice (persists via lens.svelte's setActiveLens). */
export function chooseLens(id: string) {
  wizardState.lens = id;
  setActiveLens(id);
}

/** Reset to a clean state (defensive; the wizard runs once). */
export function resetWizard() {
  wizardState.step = 0;
  wizardState.vaultPath = null;
  wizardState.useDefaultVault = true;
  wizardState.lens = "lewisian";
  wizardState.llmChoice = "skip";
  wizardState.error = null;
}
