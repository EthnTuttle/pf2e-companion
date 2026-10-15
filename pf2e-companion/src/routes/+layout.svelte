<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import { ensureLensesLoaded, lensState } from "$lib/lens.svelte";
  import { campaignState, refreshCampaigns } from "$lib/campaign.svelte";

  let { children } = $props();

  onMount(async () => {
    await Promise.all([ensureLensesLoaded(), refreshCampaigns()]);
  });

  const tabs = [
    { href: "/", icon: "🔍", label: "Search" },
    { href: "/encounter", icon: "⚔️", label: "Encounter" },
    { href: "/vault", icon: "📜", label: "Vault" },
    { href: "/aliases", icon: "🔄", label: "Aliases" },
    { href: "/miracles", icon: "📖", label: "Miracles" },
    { href: "/table", icon: "🎲", label: "Table" },
  ];
</script>

<div class="app">
  <header class="topbar">
    <a class="brand" href="/" aria-label="Home">pf2e-companion</a>
    <div class="pickers">
      <a class="about-link" href="/about" title="About this app">About</a>
      <a class="about-link" href="/chat" title="Chat (optional LLM)">Chat</a>
      <label class="picker">
        <span>Campaign</span>
        <select
          bind:value={campaignState.active}
          aria-label="Active campaign"
        >
          <option value={null}>—</option>
          {#each campaignState.campaigns as c (c.id)}
            <option value={c.id}>{c.name}</option>
          {/each}
        </select>
      </label>
      <label class="picker">
        <span>Lens</span>
        <select bind:value={lensState.active}>
          {#each lensState.manifests as l (l.id)}
            <option value={l.id}>{l.label}</option>
          {:else}
            <option value="lewisian">Lewisian</option>
          {/each}
        </select>
      </label>
    </div>
  </header>

  <main>
    {@render children?.()}
  </main>

  <nav class="tabbar">
    {#each tabs as t (t.href)}
      <a
        href={t.href}
        class:active={page.url.pathname === t.href ||
          (t.href !== "/" && page.url.pathname.startsWith(t.href))}
      >
        <span class="ico">{t.icon}</span>
        <span class="lbl">{t.label}</span>
      </a>
    {/each}
  </nav>
</div>

<style>
  :global(:root) {
    font-family:
      "Inter", system-ui, -apple-system, "Segoe UI", Roboto, Helvetica, Arial,
      sans-serif;
    color-scheme: light dark;
    --muted: color-mix(in srgb, currentColor 60%, transparent);
    --line: color-mix(in srgb, currentColor 12%, transparent);
    --bg-soft: color-mix(in srgb, currentColor 4%, transparent);
  }
  :global(body) {
    margin: 0;
  }
  :global(a) {
    color: inherit;
    text-decoration: none;
  }

  .app {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
  }

  .topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.6rem 1rem;
    border-bottom: 1px solid var(--line);
    background: color-mix(in srgb, currentColor 2%, transparent);
    backdrop-filter: blur(10px);
    position: sticky;
    top: 0;
    z-index: 5;
  }
  .brand {
    font-weight: 600;
    font-size: 0.95rem;
    text-decoration: none;
    color: inherit;
  }
  .about-link {
    font-size: 0.78rem;
    color: var(--muted);
    text-decoration: none;
    border: 1px solid var(--line);
    border-radius: 999px;
    padding: 0.2rem 0.6rem;
    line-height: 1;
  }
  .about-link:hover {
    color: inherit;
    border-color: color-mix(in srgb, currentColor 30%, transparent);
  }
  .pickers {
    display: flex;
    gap: 0.6rem;
    align-items: center;
    flex-wrap: wrap;
  }
  .picker {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.78rem;
    color: var(--muted);
  }
  .picker select {
    font: inherit;
    color: inherit;
    background: var(--bg-soft);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 0.25rem 0.4rem;
    max-width: 12rem;
  }

  main {
    flex: 1;
    width: 100%;
    max-width: 760px;
    margin: 0 auto;
    padding: 1rem 1rem 5rem;
    /* 12.3-found fix: content-box width:100% + inline padding made the
       border-box viewport+32 wide (<792px), clipping content on phones. */
    box-sizing: border-box;
  }

  .tabbar {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    display: flex;
    background: color-mix(in srgb, currentColor 5%, white 92%);
    border-top: 1px solid var(--line);
    z-index: 10;
  }
  @media (prefers-color-scheme: dark) {
    .tabbar {
      background: color-mix(in srgb, currentColor 12%, black 70%);
    }
  }
  .tabbar a {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.15rem;
    padding: 0.55rem 0.5rem 0.7rem;
    font-size: 0.7rem;
    color: var(--muted);
  }
  .tabbar .active {
    color: inherit;
  }
  .tabbar .ico {
    font-size: 1.1rem;
    line-height: 1;
  }
  .tabbar .lbl {
    line-height: 1;
  }
</style>
