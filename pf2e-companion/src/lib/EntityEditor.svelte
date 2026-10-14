<script lang="ts">
  import { goto } from "$app/navigation";
  import { untrack } from "svelte";
  import {
    addRelation,
    createEntity,
    deleteEntity,
    deleteRelation,
    listRelations,
    updateEntity,
    type CrudResult,
    type EntityDetail,
    type EntityInputDTO,
    type EntityPatchDTO,
    type RelationRow,
  } from "$lib/ipc";
  import { lensState } from "$lib/lens.svelte";
  import { campaignState, refreshCampaigns } from "$lib/campaign.svelte";
  import MarkdownEditor from "$lib/MarkdownEditor.svelte";

  let {
    mode,
    entity = null,
    initialCampaign = null,
  }: {
    mode: "new" | "edit";
    entity?: EntityDetail | null;
    initialCampaign?: string | null;
  } = $props();

  // Form state — initialized in effect to avoid "captures initial value" warnings
  let title = $state("");
  let entityType = $state("npc");
  let campaign = $state("");
  let lens = $state("lewisian");
  let licenseProvenance = $state("homebrew");
  let body = $state("");
  let extraFrontmatter = $state("");

  // Initialize form state once from props — untrack global state so init never re-runs on campaign/lens changes
  $effect.pre(() => {
    if (entity) {
      title = entity.title ?? "";
      entityType = entity.type ?? "npc";
      campaign = entity.frontmatter?.campaign_id?.toString() ?? initialCampaign ?? untrack(() => campaignState.active) ?? "";
      lens = entity.lens ?? untrack(() => lensState.active) ?? "lewisian";
      licenseProvenance = entity.license_provenance ?? "homebrew";
      body = entity.body ?? defaultBody(entityType, title);
      extraFrontmatter = JSON.stringify(stripCanonical(entity.frontmatter ?? {}), null, 2);
    } else {
      title = "";
      entityType = "npc";
      campaign = initialCampaign ?? untrack(() => campaignState.active) ?? "";
      lens = untrack(() => lensState.active) ?? "lewisian";
      licenseProvenance = "homebrew";
      body = defaultBody(entityType, title);
      extraFrontmatter = "";
    }
  });

  let saving = $state(false);
  let deleting = $state(false);
  let error = $state<string | null>(null);

  // Relations (only meaningful in edit mode)
  let relations = $state<RelationRow[]>([]);
  let newRelEdge = $state("member_of");
  let newRelTo = $state("");

  $effect(() => {
    if (mode === "edit" && entity) refreshRelations();
  });

  async function refreshRelations() {
    if (!entity) return;
    try {
      relations = await listRelations(entity.id);
    } catch (e) {
      error = String(e);
    }
  }

  function defaultBody(t: string, title: string) {
    const heading = title || "New entry";
    return `# ${heading}\n\n`;
  }

  function stripCanonical(fm: Record<string, unknown>) {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(fm)) {
      if (k === "id" || k === "campaign_id" || k === "title" || k === "type" || k === "lens" || k === "license_provenance") continue;
      out[k] = v;
    }
    return out;
  }

  function parseExtraFrontmatter(): Record<string, unknown> | null {
    const raw = extraFrontmatter.trim();
    if (!raw) return {};
    try {
      const parsed = JSON.parse(raw);
      if (typeof parsed !== "object" || Array.isArray(parsed) || parsed === null) {
        error = "extra frontmatter must be a JSON object";
        return null;
      }
      return parsed as Record<string, unknown>;
    } catch (e) {
      error = `extra frontmatter JSON: ${e}`;
      return null;
    }
  }

  async function handleSave(e: Event) {
    e.preventDefault();
    error = null;
    if (!title.trim()) {
      error = "title is required";
      return;
    }
    if (mode === "new" && !campaign.trim()) {
      error = "campaign is required for new entries";
      return;
    }
    const extra = parseExtraFrontmatter();
    if (extra === null) return;
    saving = true;
    try {
      let result: CrudResult;
      if (mode === "new") {
        const input: EntityInputDTO = {
          campaign_id: campaign,
          type: entityType,
          title,
          lens: lens || null,
          license_provenance: licenseProvenance,
          body,
          frontmatter: extra,
        };
        result = await createEntity(input);
        await refreshCampaigns(true);
      } else {
        if (!entity) throw new Error("missing entity in edit mode");
        const patch: EntityPatchDTO = {
          title: title !== entity.title ? title : null,
          lens: lens !== entity.lens ? lens : null,
          license_provenance:
            licenseProvenance !== entity.license_provenance
              ? licenseProvenance
              : null,
          body,
          frontmatter: extra,
        };
        result = await updateEntity(entity.id, patch);
      }
      await goto(`/entity/${encodeURIComponent(result.id)}`);
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  async function handleDelete() {
    if (!entity) return;
    if (!confirm(`Delete ${entity.title}? This cannot be undone.`)) return;
    deleting = true;
    error = null;
    try {
      await deleteEntity(entity.id);
      await goto("/vault");
    } catch (e) {
      error = String(e);
      deleting = false;
    }
  }

  async function handleAddRelation(e: Event) {
    e.preventDefault();
    if (!entity || !newRelTo.trim()) return;
    try {
      await addRelation(entity.id, newRelEdge, newRelTo.trim());
      await refreshRelations();
      newRelTo = "";
    } catch (e) {
      error = String(e);
    }
  }

  async function handleDeleteRelation(rel: RelationRow) {
    try {
      await deleteRelation(rel.from_id, rel.edge_type, rel.to_id);
      await refreshRelations();
    } catch (e) {
      error = String(e);
    }
  }

  const entityTypeOptions = [
    "npc",
    "creature",
    "location",
    "faction",
    "family",
    "item",
    "quest",
    "event",
    "session",
    "note",
  ];

  const licenseOptions = [
    { id: "homebrew", label: "Homebrew (default)" },
    { id: "orc", label: "ORC" },
    { id: "community-use", label: "Community Use (free distribution only)" },
    { id: "proprietary", label: "Proprietary" },
  ];
</script>

<form class="editor" onsubmit={handleSave}>
  <header>
    <button
      type="button"
      class="back"
      onclick={() => history.length > 1 ? history.back() : goto("/vault")}
    >
      ← Back
    </button>
    <h1>{mode === "new" ? "New entry" : entity?.title ?? "Edit entry"}</h1>
  </header>

  {#if error}
    <p class="err" role="alert">{error}</p>
  {/if}

  <div class="grid">
    <label class="span" for="title-input">
      <span class="lbl">Title</span>
      <input id="title-input" type="text" bind:value={title} required />
    </label>

    <label for="type-select">
      <span class="lbl">Type</span>
      <select id="type-select" bind:value={entityType} disabled={mode === "edit"}>
        {#each entityTypeOptions as t (t)}
          <option value={t}>{t}</option>
        {/each}
      </select>
    </label>

    <label for="campaign-select">
      <span class="lbl">Campaign</span>
      <select id="campaign-select" bind:value={campaign} disabled={mode === "edit"}>
        <option value="">—</option>
        {#each campaignState.campaigns as c (c.id)}
          <option value={c.id}>{c.name}</option>
        {/each}
      </select>
    </label>

    <label for="lens-select">
      <span class="lbl">Lens</span>
      <select id="lens-select" bind:value={lens}>
        <option value="">(unspecified)</option>
        {#each lensState.manifests as l (l.id)}
          <option value={l.id}>{l.label}</option>
        {/each}
      </select>
    </label>

    <label for="license-select">
      <span class="lbl">License</span>
      <select id="license-select" bind:value={licenseProvenance}>
        {#each licenseOptions as o (o.id)}
          <option value={o.id}>{o.label}</option>
        {/each}
      </select>
    </label>
  </div>

  <section>
    <h2>Body</h2>
    <MarkdownEditor bind:value={body} placeholder="Markdown body" />
  </section>

  <section>
    <h2>Extra frontmatter (JSON)</h2>
    <p class="muted small">
      Anything beyond title/type/campaign/lens/license. Common keys:
      <code>status</code>, <code>relations</code>, <code>sources</code>,
      <code>mechanical</code>.
    </p>
    <textarea id="extra-frontmatter" bind:value={extraFrontmatter} spellcheck="false" rows="6"></textarea>
  </section>

  {#if mode === "edit" && entity}
    <section>
      <h2>Relations</h2>
      {#if relations.length}
        <ul class="rels">
          {#each relations as r (r.from_id + r.edge_type + r.to_id)}
            <li>
              <span class="dir">
                {r.from_id === entity.id ? "→" : "←"}
              </span>
              <span class="edge">{r.edge_type}</span>
              <span class="other">
                {r.from_id === entity.id ? r.to_id : r.from_id}
              </span>
              <button
                type="button"
                class="x"
                aria-label="Remove relation"
                onclick={() => handleDeleteRelation(r)}
              >×</button>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="muted small">No relations.</p>
      {/if}

      <div class="new-rel">
        <label for="rel-edge-input">
          <span class="lbl" style="position:absolute;width:1px;height:1px;overflow:hidden">Edge type</span>
          <input
            id="rel-edge-input"
            type="text"
            placeholder="edge (member_of, located_in, …)"
            bind:value={newRelEdge}
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                handleAddRelation(e);
              }
            }}
          />
        </label>
        <label for="rel-target-input">
          <span class="lbl" style="position:absolute;width:1px;height:1px;overflow:hidden">Target ID</span>
          <input
            id="rel-target-input"
            type="text"
            placeholder="target id (e.g. main:faction/house-velerian)"
            bind:value={newRelTo}
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                handleAddRelation(e);
              }
            }}
          />
        </label>
        <button type="button" onclick={handleAddRelation}>Add</button>
      </div>
    </section>
  {/if}

  <footer>
    <button type="submit" class="primary" disabled={saving}>
      {saving ? "Saving…" : mode === "new" ? "Create" : "Save"}
    </button>
    {#if mode === "edit"}
      <button
        type="button"
        class="danger"
        onclick={handleDelete}
        disabled={deleting}
      >
        {deleting ? "Deleting…" : "Delete"}
      </button>
    {/if}
  </footer>
</form>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  header {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }
  .back {
    align-self: flex-start;
    font: inherit;
    background: transparent;
    border: none;
    color: var(--muted);
    cursor: pointer;
    padding: 0.25rem 0;
    font-size: 0.85rem;
  }
  h1 {
    margin: 0;
    font-size: 1.4rem;
    font-weight: 600;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(11rem, 1fr));
    gap: 0.7rem;
  }
  .grid .span {
    grid-column: 1 / -1;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.85rem;
  }
  .lbl {
    color: var(--muted);
    font-size: 0.75rem;
  }
  input[type="text"],
  select,
  textarea {
    font: inherit;
    padding: 0.5rem 0.7rem;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    color: inherit;
  }
  textarea {
    font-family: "SF Mono", Menlo, Monaco, Consolas, monospace;
    font-size: 0.85rem;
    resize: vertical;
  }
  select[disabled] {
    opacity: 0.6;
  }

  section h2 {
    margin: 0 0 0.4rem;
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
    font-weight: 600;
  }
  .small {
    font-size: 0.78rem;
  }
  .muted {
    color: var(--muted);
  }
  .muted code {
    background: var(--bg-soft);
    padding: 0.05rem 0.3rem;
    border-radius: 4px;
    font-size: 0.78rem;
  }

  .rels {
    list-style: none;
    padding: 0;
    margin: 0 0 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }
  .rels li {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    padding: 0.4rem 0.6rem;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--bg-soft);
    font-size: 0.83rem;
  }
  .dir {
    color: var(--muted);
    width: 1rem;
  }
  .edge {
    font-family: "SF Mono", monospace;
    font-size: 0.78rem;
    color: var(--muted);
  }
  .other {
    flex: 1;
    overflow-wrap: anywhere;
  }
  .x {
    font: inherit;
    border: none;
    background: transparent;
    cursor: pointer;
    color: var(--muted);
    padding: 0 0.3rem;
    line-height: 1;
  }
  .x:hover {
    color: #c33;
  }

  .new-rel {
    display: grid;
    grid-template-columns: 12rem 1fr auto;
    gap: 0.4rem;
    align-items: end;
  }
  .new-rel label {
    gap: 0.2rem;
  }
  .new-rel input {
    font-size: 0.85rem;
  }
  .new-rel button {
    font: inherit;
    padding: 0.4rem 0.85rem;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: color-mix(in srgb, currentColor 10%, transparent);
    color: inherit;
    cursor: pointer;
  }
  @media (max-width: 540px) {
    .new-rel {
      grid-template-columns: 1fr;
    }
  }

  footer {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.5rem;
    padding-top: 0.6rem;
    border-top: 1px solid var(--line);
  }
  footer button {
    font: inherit;
    padding: 0.55rem 1rem;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--bg-soft);
    color: inherit;
    cursor: pointer;
  }
  footer .primary {
    background: color-mix(in srgb, currentColor 12%, transparent);
    border-color: color-mix(in srgb, currentColor 30%, transparent);
    font-weight: 500;
  }
  footer .danger {
    color: #c33;
    margin-left: auto;
    border-color: color-mix(in srgb, #c33 30%, var(--line));
  }
  footer button[disabled] {
    opacity: 0.6;
    cursor: progress;
  }

  .err {
    color: #c33;
    padding: 0.5rem 0.75rem;
    border: 1px solid #c33;
    border-radius: 6px;
    background: rgba(204, 51, 51, 0.05);
  }
</style>