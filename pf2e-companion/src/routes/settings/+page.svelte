<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { diagnosticsGet, type DiagnosticsInfo } from "$lib/ipc";
  import { formatDiagnosticsMarkdown } from "$lib/diagnostics";

  let info = $state<DiagnosticsInfo | null>(null);
  let blob = $state("");
  let loading = $state(false);
  let copied = $state(false);
  let copyFailed = $state(false);
  let error = $state<string | null>(null);

  onMount(async () => {
    try {
      info = await diagnosticsGet();
    } catch {
      // The count stays hidden; Copy still retries on demand.
    }
  });

  async function copyDiagnostics() {
    if (loading) return;
    loading = true;
    error = null;
    copied = false;
    copyFailed = false;
    try {
      info = await diagnosticsGet();
      blob = formatDiagnosticsMarkdown(info);
      await navigator.clipboard.writeText(blob);
      copied = true;
      setTimeout(() => (copied = false), 4000);
    } catch (e) {
      if (blob) {
        // Clipboard unavailable (webview denial, mobile) — reveal the
        // manually selectable fallback with the same text.
        copyFailed = true;
      } else {
        error = String(e);
      }
    } finally {
      loading = false;
    }
  }
</script>

<header class="hero">
  <button
    type="button"
    class="back"
    onclick={() => (history.length > 1 ? history.back() : goto("/"))}
  >
    ← Back
  </button>
  <h1>Settings</h1>
  <p class="tagline">
    Copy diagnostics lives here; LLM provider and eval-harness
    configuration live one level down.
  </p>
</header>

<section class="card">
  <h2>Diagnostics</h2>
  <p>
    <strong>Copy diagnostics</strong> puts a paste-ready summary of the last
    session's activity on your clipboard — app version, OS, active session,
    and the most recent log lines. No telemetry: nothing leaves this machine
    until you paste it somewhere yourself (a GitHub issue, a message to the
    maintainer).
  </p>
  <p class="muted">
    {#if info}
      Last {info.lines.length} events captured{#if info.session_id} ·
        active session {info.session_id}{/if}.
    {:else}
      Log capture count unavailable.
    {/if}
  </p>
  <div class="actions">
    <button type="button" class="primary" onclick={copyDiagnostics} disabled={loading}>
      {loading ? "Collecting…" : "Copy diagnostics"}
    </button>
    {#if copied}
      <span class="ok">Copied — paste into a GitHub issue.</span>
    {/if}
  </div>
  {#if error}
    <p class="err">{error}</p>
  {/if}
  {#if copyFailed}
    <details class="fallback">
      <summary>Clipboard unavailable — copy manually</summary>
      <textarea readonly rows="12" aria-label="Diagnostics text" bind:value={blob}></textarea>
    </details>
  {/if}
</section>

<section class="card">
  <h2>More settings</h2>
  <ul class="links">
    <li><a href="/settings/llm">LLM settings</a> — optional chat assistance (off by default).</li>
    <li><a href="/settings/eval">Eval suite</a> — run the prompt expectations harness.</li>
  </ul>
</section>

<style>
  .hero {
    margin-bottom: 1rem;
  }
  .back {
    align-self: flex-start;
    font: inherit;
    background: transparent;
    border: none;
    color: var(--muted);
    cursor: pointer;
    padding: 0.25rem 0;
    margin-bottom: 0.4rem;
    font-size: 0.85rem;
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
    margin: 0 0 1.5rem;
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
  .card p {
    margin: 0.5rem 0;
  }
  .muted {
    color: var(--muted);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin-top: 0.6rem;
  }
  .ok {
    color: var(--ok, #16a34a);
  }
  .err {
    color: #dc2626;
    margin: 0.5rem 0 0;
  }

  .fallback {
    margin-top: 0.75rem;
  }
  .fallback summary {
    cursor: pointer;
    color: var(--muted);
  }
  .fallback textarea {
    width: 100%;
    margin-top: 0.5rem;
    font: 0.78rem/1.4 ui-monospace, monospace;
    box-sizing: border-box;
  }

  .links {
    list-style: none;
    margin: 0.5rem 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
</style>
