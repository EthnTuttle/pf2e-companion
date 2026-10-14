<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import {
    appRestart,
    llmConfigure,
    wizardComplete,
    wizardStatus,
    wizardValidateVault,
    type WizardStatus,
  } from "$lib/ipc";
  import { ensureLensesLoaded, lensState } from "$lib/lens.svelte";
  import {
    back,
    chooseLens,
    next,
    validateTypedVaultPath,
    wizardProgress,
    wizardState,
  } from "$lib/wizard.svelte";

  let status = $state<WizardStatus | null>(null);
  let unavailable = $state(false);
  let typedPath = $state("");
  let llmModel = $state("qwen3:4b");
  let llmApiKey = $state("");
  let finishing = $state(false);
  let restarting = $state(false);
  let finishError = $state<string | null>(null);

  onMount(async () => {
    try {
      status = await wizardStatus();
    } catch {
      // Direct-nav without Tauri IPC (browser client) — the wizard can't
      // persist anything, so degrade to a notice instead of crashing.
      unavailable = true;
    }
    await ensureLensesLoaded();
  });

  async function chooseVaultFolder() {
    wizardState.error = null;
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const picked = await open({ directory: true });
      if (picked === null) return; // cancelled — keep the previous choice
      wizardState.vaultPath = await wizardValidateVault(String(picked));
      wizardState.useDefaultVault = false;
    } catch (e) {
      wizardState.error = String(e);
    }
  }

  function keepDefaultVault() {
    wizardState.useDefaultVault = true;
    wizardState.vaultPath = null;
    wizardState.error = null;
  }

  async function finish() {
    if (finishing) return;
    finishing = true;
    finishError = null;
    try {
      // Configure the provider first (same path as /settings/llm) so a
      // failure here leaves the wizard open with the error visible.
      if (wizardState.llmChoice !== "skip") {
        await llmConfigure(
          {
            provider: wizardState.llmChoice,
            model: llmModel,
            base_url: wizardState.llmChoice === "ollama" ? "http://localhost:11434" : null,
          },
          wizardState.llmChoice === "anthropic" ? llmApiKey || null : null,
        );
      }
      const result = await wizardComplete(
        wizardState.useDefaultVault ? null : wizardState.vaultPath,
      );
      if (result.restart_required) {
        restarting = true;
        appRestart(); // fire-and-forget — the process exits
        return;
      }
      goto("/");
    } catch (e) {
      finishError = String(e);
      finishing = false;
    }
  }

  async function skipWizard() {
    try {
      await wizardComplete(null);
    } catch {
      // Best effort — still leave.
    }
    goto("/");
  }
</script>

<header class="hero">
  <h1>Welcome to pf2e-companion</h1>
  <p class="tagline">
    Three quick choices and you're at the table. Everything is optional and
    changeable later in Settings.
  </p>
</header>

{#if unavailable}
  <section class="card">
    <h2>Wizard unavailable</h2>
    <p>
      This page needs the app's native backend. Close this browser tab and
      reopen the pf2e-companion app to continue setup.
    </p>
  </section>
{:else if restarting}
  <section class="card">
    <h2>Vault location set</h2>
    <p>Restarting the app so the new vault folder takes effect…</p>
  </section>
{:else}
  <section class="card">
    <h2>{wizardProgress(wizardState.step)}</h2>

    {#if wizardState.step === 0}
      <fieldset>
        <legend>Where should your campaign vault live?</legend>
        <p class="muted">
          Campaigns, entities, and session saves live here as plain markdown —
          easy to back up, sync, and read outside the app.
        </p>
        <label class="option">
          <input
            type="radio"
            name="vault"
            checked={wizardState.useDefaultVault}
            onchange={keepDefaultVault}
          />
          <span>
            <strong>Use the default vault folder</strong>
            {#if status}<span class="muted"> — {status.default_vault}</span>{/if}
          </span>
        </label>
        <label class="option">
          <input
            type="radio"
            name="vault"
            checked={!wizardState.useDefaultVault && wizardState.vaultPath !== null}
            onchange={chooseVaultFolder}
          />
          <span><strong>Choose a folder…</strong> (mainly for desktop)</span>
        </label>
        {#if wizardState.vaultPath && !wizardState.useDefaultVault}
          <p class="ok">{wizardState.vaultPath}</p>
        {/if}
        <details>
          <summary>Type a path instead</summary>
          <div class="typed">
            <input
              type="text"
              bind:value={typedPath}
              placeholder="/home/gm/my-vault"
              aria-label="Vault folder path"
            />
            <button type="button" onclick={() => validateTypedVaultPath(typedPath)}>
              Validate
            </button>
          </div>
        </details>
        {#if wizardState.error}
          <p class="err">{wizardState.error}</p>
        {/if}
      </fieldset>
    {:else if wizardState.step === 1}
      <fieldset>
        <legend>Which lens do you build worlds through?</legend>
        <p class="muted">
          The lens shapes reference content and theological framing. You can
          switch it any time from the top bar.
        </p>
        {#each lensState.manifests as l (l.id)}
          <label class="option">
            <input
              type="radio"
              name="lens"
              value={l.id}
              checked={wizardState.lens === l.id}
              onchange={() => chooseLens(l.id)}
            />
            <span><strong>{l.label}</strong> — {l.description}</span>
          </label>
        {:else}
          <p class="muted">Lens list unavailable — the default (Lewisian) is used.</p>
        {/each}
      </fieldset>
    {:else}
      <fieldset>
        <legend>Optional: AI chat assistance</legend>
        <p class="muted">
          Off by default — the app is fully usable without it. If you turn it
          on now, you can manage it later under
          <a href="/settings/llm">Settings → LLM</a>.
        </p>
        <label class="option">
          <input
            type="radio"
            name="llm"
            checked={wizardState.llmChoice === "skip"}
            onchange={() => (wizardState.llmChoice = "skip")}
          />
          <span><strong>Skip</strong> — keep AI features off</span>
        </label>
        <label class="option">
          <input
            type="radio"
            name="llm"
            checked={wizardState.llmChoice === "ollama"}
            onchange={() => (wizardState.llmChoice = "ollama")}
          />
          <span><strong>Ollama</strong> — local model at http://localhost:11434</span>
        </label>
        <label class="option">
          <input
            type="radio"
            name="llm"
            checked={wizardState.llmChoice === "anthropic"}
            onchange={() => (wizardState.llmChoice = "anthropic")}
          />
          <span><strong>Anthropic</strong> — bring your own API key</span>
        </label>
        {#if wizardState.llmChoice !== "skip"}
          <label class="field">
            <span>Model</span>
            <input
              type="text"
              bind:value={llmModel}
              aria-label="LLM model"
            />
          </label>
          {#if wizardState.llmChoice === "anthropic"}
            <label class="field">
              <span>API key</span>
              <input
                type="password"
                bind:value={llmApiKey}
                aria-label="Anthropic API key"
              />
            </label>
          {/if}
        {/if}
      </fieldset>
    {/if}

    <div class="actions">
      {#if wizardState.step > 0}
        <button type="button" onclick={back}>Back</button>
      {/if}
      {#if wizardState.step < 2}
        <button
          type="button"
          class="primary"
          onclick={next}
          disabled={wizardState.step === 0 && !wizardState.useDefaultVault && !wizardState.vaultPath}
        >
          Next
        </button>
      {:else}
        <button type="button" class="primary" onclick={finish} disabled={finishing}>
          {finishing ? "Finishing…" : "Finish"}
        </button>
      {/if}
    </div>
    {#if finishError}
      <p class="err">{finishError}</p>
    {/if}
  </section>

  <footer class="skip">
    <button type="button" class="linklike" onclick={skipWizard}>
      Skip wizard — set everything up later
    </button>
  </footer>
{/if}

<style>
  .hero {
    margin-bottom: 1rem;
  }
  h1 {
    margin: 0.1rem 0 0;
    font-size: 1.4rem;
    font-weight: 600;
  }
  .tagline {
    margin: 0.2rem 0 0;
    color: var(--muted);
    font-size: 0.85rem;
  }

  .card {
    margin: 0 0 1rem;
    padding: 0.85rem 1rem;
    border-radius: 12px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    font-size: 0.85rem;
    line-height: 1.5;
  }
  .card h2 {
    margin: 0 0 0.4rem;
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
  }
  fieldset {
    border: none;
    margin: 0;
    padding: 0;
  }
  legend {
    font-weight: 600;
    padding: 0;
    margin-bottom: 0.3rem;
  }
  .muted {
    color: var(--muted);
  }
  .option {
    display: flex;
    gap: 0.5rem;
    align-items: flex-start;
    margin: 0.45rem 0;
    cursor: pointer;
  }
  .option input {
    margin-top: 0.2rem;
  }
  .field {
    display: block;
    margin: 0.5rem 0;
  }
  .field span {
    display: block;
    color: var(--muted);
    font-size: 0.78rem;
  }
  .field input {
    font: inherit;
    color: inherit;
    background: var(--bg-soft);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 0.25rem 0.4rem;
    width: 100%;
    box-sizing: border-box;
  }
  .typed {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.4rem;
  }
  .typed input {
    flex: 1;
    font: inherit;
    color: inherit;
    background: var(--bg-soft);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 0.25rem 0.4rem;
  }
  .typed button,
  .actions button {
    font: inherit;
    color: inherit;
    background: var(--bg-soft);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 0.3rem 0.8rem;
    cursor: pointer;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.6rem;
    margin-top: 0.8rem;
  }
  .actions .primary {
    background: color-mix(in srgb, currentColor 10%, transparent);
    font-weight: 600;
  }
  .actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .ok {
    color: var(--ok, #16a34a);
  }
  .err {
    color: #dc2626;
    margin: 0.5rem 0 0;
  }
  details {
    margin-top: 0.4rem;
  }
  details summary {
    cursor: pointer;
    color: var(--muted);
    font-size: 0.78rem;
  }
  .skip {
    text-align: center;
  }
  .linklike {
    font: inherit;
    background: none;
    border: none;
    color: var(--muted);
    text-decoration: underline;
    cursor: pointer;
    font-size: 0.78rem;
  }
</style>
