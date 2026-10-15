<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import { goto } from "$app/navigation";
  import { marked } from "marked";
  import { getEntity, type EntityDetail } from "$lib/ipc";
  import { packArtPath } from "$lib/images";
  import EntityImage from "$lib/EntityImage.svelte";
  import Statblock from "$lib/Statblock.svelte";
  import Sources from "$lib/Sources.svelte";

  let entity = $state<EntityDetail | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);

  // Imagery v1 hero (DEDUCTION §4.3.1): adventure/plane art renders as a
  // banner above `.layout`; every other arted entity gets a right-floating
  // portrait card inside the prose. Unpromoted art resolves to null and the
  // page renders exactly as today.
  let artPath = $derived(
    packArtPath(
      entity?.lens,
      typeof entity?.frontmatter?.image === "string"
        ? entity.frontmatter.image
        : null,
    ),
  );
  let artAlt = $derived(
    typeof entity?.frontmatter?.image_alt === "string"
      ? entity.frontmatter.image_alt
      : "",
  );
  let artIsBanner = $derived(
    entity?.type === "adventure" || entity?.type === "plane",
  );

  // Convert [[id]] wikilinks into <a href="/entity/{id}"> before passing to
  // marked. Keep it simple — the brackets are exact, no escapes.
  const wikiLink = (md: string) =>
    md.replace(
      /\[\[([^\]\|]+?)\]\]/g,
      (_match, id) =>
        `[${id.replaceAll(":", " · ")}](/entity/${encodeURIComponent(id)})`,
    );

  let bodyHtml = $derived(
    entity?.body
      ? (marked.parse(wikiLink(entity.body), { async: false }) as string)
      : "",
  );

  let sources = $derived(() => {
    const s = entity?.frontmatter?.sources;
    return Array.isArray(s) ? (s as string[]) : [];
  });

  async function load(id: string) {
    loading = true;
    error = null;
    entity = null;
    try {
      entity = await getEntity(id);
      if (!entity) error = `No entity with id "${id}".`;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void load(decodeURIComponent(page.params.id ?? ""));
  });
</script>

<header class="hero">
  <button
    type="button"
    class="back"
    onclick={() => history.length > 1 ? history.back() : goto("/")}
  >
    ← Back
  </button>
  {#if entity}
    <h1>{entity.title}</h1>
    <div class="meta">
      <span class="type">{entity.type}</span>
      {#if entity.lens}<span class="lens">lens: {entity.lens}</span>{/if}
      <span class="lic">license: {entity.license_provenance}</span>
      <span class="src">{entity.source}</span>
    </div>
  {/if}
</header>

{#if loading}
  <p class="muted">Loading…</p>
{:else if error}
  <p class="err">{error}</p>
{:else if entity}
  {#if artIsBanner && artPath}
    <EntityImage path={artPath} alt={artAlt} class="art-banner" />
  {/if}
  <article class="layout">
    <div class="body prose">
      {#if !artIsBanner && artPath}
        <EntityImage path={artPath} alt={artAlt} class="art-portrait" />
      {/if}
      {@html bodyHtml}
    </div>

    <div class="rail">
      {#if entity.statblock}
        <Statblock statblock={entity.statblock} />
      {/if}

      {#if sources().length}
        <Sources sources={sources()} />
      {/if}
    </div>
  </article>
{/if}

<style>
  /* Imagery v1 (DEDUCTION §4.3.1): global because the <img> lives inside
     EntityImage. */
  :global(.art-banner) {
    display: block;
    width: 100%;
    max-height: 15rem;
    object-fit: cover;
    border: 1px solid var(--line);
    border-radius: 0.5rem;
    margin-bottom: 1rem;
  }
  :global(.art-portrait) {
    float: right;
    width: min(16rem, 45%);
    margin: 0 0 0.75rem 1rem;
    border: 1px solid var(--line);
    border-radius: 0.5rem;
  }
  .hero {
    margin-bottom: 1rem;
  }
  .back {
    font: inherit;
    background: transparent;
    border: none;
    color: var(--muted);
    cursor: pointer;
    padding: 0.25rem 0;
    margin-bottom: 0.4rem;
    font-size: 0.85rem;
  }
  .back:hover {
    color: inherit;
  }
  h1 {
    margin: 0.1rem 0 0;
    font-size: 1.4rem;
    font-weight: 600;
  }
  .meta {
    margin-top: 0.4rem;
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    font-size: 0.7rem;
    color: var(--muted);
  }
  .meta span {
    padding: 0.1rem 0.45rem;
    border: 1px solid var(--line);
    border-radius: 999px;
  }
  .meta .type {
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .err {
    color: #c33;
  }
  .muted {
    color: var(--muted);
  }

  .layout {
    display: grid;
    grid-template-columns: 1fr;
    gap: 1.25rem;
  }
  @media (min-width: 760px) {
    .layout {
      grid-template-columns: minmax(0, 1fr) 18rem;
    }
  }

  .prose {
    line-height: 1.55;
  }
  .prose :global(h1) {
    display: none;
  }
  .prose :global(h2) {
    margin-top: 1.5rem;
    font-size: 1.05rem;
    border-bottom: 1px solid var(--line);
    padding-bottom: 0.2rem;
  }
  .prose :global(h3) {
    font-size: 0.95rem;
    margin-top: 1.2rem;
  }
  .prose :global(p),
  .prose :global(ul),
  .prose :global(ol) {
    margin-block: 0.7rem;
  }
  .prose :global(table) {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.85rem;
    margin-block: 1rem;
  }
  .prose :global(th),
  .prose :global(td) {
    border-bottom: 1px solid var(--line);
    padding: 0.4rem 0.55rem;
    text-align: left;
  }
  .prose :global(th) {
    font-weight: 500;
    color: var(--muted);
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }
  .prose :global(code) {
    background: var(--bg-soft);
    padding: 0.05rem 0.3rem;
    border-radius: 4px;
    font-size: 0.85em;
  }
  .prose :global(blockquote) {
    border-left: 3px solid var(--line);
    margin: 1rem 0;
    padding: 0.4rem 0.85rem;
    color: var(--muted);
    font-style: italic;
  }
  .prose :global(a) {
    color: inherit;
    text-decoration: underline;
    text-decoration-color: color-mix(in srgb, currentColor 30%, transparent);
    text-underline-offset: 0.15em;
  }
  .prose :global(a:hover) {
    text-decoration-color: currentColor;
  }

  .rail {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
</style>
