<script lang="ts">
  // Additive image slot (docs/imagery/DEDUCTION.md §4.3): renders nothing
  // until the art path resolves, so every host keeps its shipped fallback
  // when an asset is unpromoted. Css classes come from the caller via
  // `class`; each seam styles its own slot.
  import { resolveArtUrl } from "$lib/images";

  let {
    path,
    alt,
    class: klass = "",
  }: { path: string | null | undefined; alt: string; class?: string } = $props();

  let url = $state<string | null>(null);

  $effect(() => {
    let live = true;
    url = null;
    void resolveArtUrl(path).then((u) => {
      if (live) url = u;
    });
    return () => {
      live = false;
    };
  });
</script>

{#if url}
  <img src={url} {alt} class={klass} loading="lazy" />
{/if}
