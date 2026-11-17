<script lang="ts">
  import type { ProjectView } from "./lib/api";
  import { api } from "./lib/api";
  import { getDef, type Catalog, type Op } from "./lib/graph";

  let {
    path,
    catalog,
    view,
    selected,
    apply,
  }: { path: string; catalog: Catalog; view: ProjectView; selected: string | null; apply: (ops: Op[]) => Promise<void> } = $props();

  // null: follow the selection; true/false: opened or closed by hand until the selection changes.
  let pinned: boolean | null = $state(null);
  $effect(() => {
    void selected;
    pinned = null;
  });
  const open = $derived(pinned ?? selected !== null);

  const settings = $derived({ width: 32, height: 32, frames: 1, fps: 12, ...(view.project.settings ?? {}) });
  const node = $derived(selected ? view.project.nodes?.[selected] : undefined);
  const def = $derived(node ? getDef(catalog, node.type) : undefined);
  const outputNode = $derived(Object.entries(view.project.nodes ?? {}).find(([, n]) => n.type === "output")?.[0] ?? null);
  const previewNode = $derived(node && def?.outputs[0]?.type === "surface" ? selected : outputNode);
  let zoom = $state(8);
  let frame = $state(0);
  let previewError = $state(false);
</script>

<aside class="panel" class:collapsed={!open}>
  <button class="toggle" aria-expanded={open} aria-label={open ? "close panel" : "open panel"} onclick={() => (pinned = !open)}>
    {open ? "›" : "‹"}
  </button>
  {#if open}
    <section class="preview">
      <h3>Preview <span class="muted">{previewNode ?? ""}</span></h3>
      <div class="row">
        <label>zoom <input type="range" min="1" max="32" bind:value={zoom} /></label>
        <label>frame <input type="number" min="0" max={settings.frames - 1} bind:value={frame} /></label>
      </div>
      {#if previewNode}
        <div class="canvas">
          <img
            class="pixel"
            src={api.renderUrl(path, previewNode, frame, view.keys[previewNode] ?? "")}
            alt="preview of {previewNode}"
            style="zoom: {zoom}"
            onerror={() => (previewError = true)}
            onload={() => (previewError = false)}
          />
        </div>
        {#if previewError}<p class="error">Cannot render this node; see its errors.</p>{/if}
      {:else}
        <p class="muted">Add an output node or select a node.</p>
      {/if}
    </section>
    <section>
      <h3>Project</h3>
      {#each ["width", "height", "frames", "fps"] as k (k)}
        <label class="row">
          <span class="name">{k}</span>
          <input
            type="number"
            min="1"
            value={settings[k as keyof typeof settings]}
            onchange={(e) => apply([{ op: "set_settings", [k]: Number(e.currentTarget.value) }])}
          />
        </label>
      {/each}
    </section>
  {/if}
</aside>

<style>
  .panel { position: relative; width: 340px; border-left: 1px solid var(--line); background: var(--surface); overflow: auto; transition: width var(--m-panel) var(--ease); }
  .panel.collapsed { width: 36px; overflow: hidden; }
  .toggle { position: sticky; top: 8px; margin: 8px 0 0 4px; width: 28px; height: 28px; padding: 0; z-index: 1; }
  section { padding: 4px 16px 12px; border-bottom: 1px solid var(--line); }
  h3 { margin: 4px 0 8px; font-weight: 500; font-size: 13px; }
  .row { display: flex; gap: 12px; align-items: center; margin: 6px 0; flex-wrap: wrap; }
  .name { width: 96px; font-family: var(--font-mono); font-size: 12px; color: var(--muted); }
  .canvas { overflow: auto; max-height: 50vh; background: repeating-conic-gradient(#232b29 0 25%, #2a3331 0 50%) 0 0 / 16px 16px; padding: 4px; border-radius: var(--r-sm); }
</style>
