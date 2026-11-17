<script lang="ts">
  import type { ProjectView } from "./lib/api";
  import { api } from "./lib/api";
  import { getDef, literalOp, parseRef, resetOp, type Catalog, type Op } from "./lib/graph";
  import Widget from "./Widget.svelte";

  let {
    path,
    catalog,
    view,
    selected = $bindable(),
    apply,
  }: { path: string; catalog: Catalog; view: ProjectView; selected: string | null; apply: (ops: Op[]) => Promise<void> } = $props();

  const settings = $derived({ width: 32, height: 32, frames: 1, fps: 12, ...(view.project.settings ?? {}) });
  const node = $derived(selected ? view.project.nodes?.[selected] : undefined);
  const def = $derived(node ? getDef(catalog, node.type) : undefined);
  const outputNode = $derived(Object.entries(view.project.nodes ?? {}).find(([, n]) => n.type === "output")?.[0] ?? null);
  const previewNode = $derived(node && def?.outputs[0]?.type === "surface" ? selected : outputNode);
  let zoom = $state(8);
  let frame = $state(0);
  let previewError = $state(false);

  async function rename(e: Event & { currentTarget: HTMLInputElement }) {
    const to = e.currentTarget.value.trim();
    if (!selected || to === selected) return;
    const from = selected;
    selected = to;
    await apply([{ op: "rename_node", id: from, new_id: to }]);
    if (!view.project.nodes?.[to]) selected = from;
  }
</script>

<aside>
  {#if node && def && selected}
    <section>
      <h3>{def.type} <span class="muted">{def.category}</span></h3>
      <p class="muted">{def.description}</p>
      <label class="row">id <input value={selected} onchange={rename} /></label>
      <label class="row">label <input value={node.label ?? ""} onchange={(e) => apply([{ op: "set_label", id: selected, label: e.currentTarget.value || null }])} /></label>
      {#each def.inputs as input (input.name)}
        {@const raw = node.inputs?.[input.name]}
        {@const ref = parseRef(raw)}
        <div class="row" title={input.description}>
          <span class="name">{input.name}{input.required ? " *" : ""}</span>
          <span class="widget">
            {#if ref}
              <span class="ref">← {ref.node}{ref.output ? "." + ref.output : ""}</span>
              <button onclick={() => apply([resetOp(selected!, input.name)])}>disconnect</button>
            {:else if input.type === "surface"}
              <span class="muted">connect in the graph</span>
            {:else}
              <Widget
                def={input}
                value={raw ?? input.default}
                size={[settings.width, settings.height]}
                change={(v) => apply([literalOp(selected!, input.name, v)])}
              />
              {#if raw !== undefined}<button title="reset to default" onclick={() => apply([resetOp(selected!, input.name)])}>↺</button>{/if}
            {/if}
          </span>
        </div>
      {/each}
    </section>
  {:else}
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
      <p class="muted">Select a node to edit it. Delete nodes and edges with Backspace.</p>
    </section>
  {/if}
  <section class="preview">
    <h3>Preview {previewNode ? `· ${previewNode}` : ""}</h3>
    <div class="row">
      <label>zoom <input type="range" min="1" max="32" bind:value={zoom} /></label>
      <label>frame <input type="number" min="0" max={settings.frames - 1} bind:value={frame} /></label>
    </div>
    {#if previewNode}
      <div class="canvas">
        <img
          class="pixel"
          src={api.renderUrl(path, previewNode, frame, view.keys[previewNode] ?? "")}
          alt="preview"
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
</aside>

<style>
  aside { border-left: 1px solid var(--line); background: var(--panel); overflow: auto; display: flex; flex-direction: column; }
  section { padding: 8px 12px; border-bottom: 1px solid var(--line); }
  h3 { margin: 4px 0; }
  .row { display: flex; gap: 8px; align-items: center; margin: 4px 0; flex-wrap: wrap; }
  .name { width: 120px; flex-shrink: 0; font-family: ui-monospace, monospace; font-size: 12px; }
  .widget { display: flex; gap: 4px; align-items: center; flex-wrap: wrap; }
  .ref { color: var(--accent); }
  .canvas { overflow: auto; max-height: 50vh; background: repeating-conic-gradient(#333 0 25%, #444 0 50%) 0 0 / 16px 16px; padding: 4px; }
</style>
