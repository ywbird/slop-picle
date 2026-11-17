<script lang="ts">
  import type { ProjectView } from "./lib/api";
  import { getDef, literalOp, parseRef, resetOp, type Catalog, type Op } from "./lib/graph";
  import Widget from "./Widget.svelte";
  import { dragOffset } from "./lib/drag";

  let {
    catalog,
    view,
    id = $bindable(),
    x,
    y,
    apply,
    close,
  }: {
    catalog: Catalog;
    view: ProjectView;
    id: string;
    x: number;
    y: number;
    apply: (ops: Op[]) => Promise<void>;
    close: () => void;
  } = $props();

  const node = $derived(view.project.nodes?.[id]);
  const def = $derived(node ? getDef(catalog, node.type) : undefined);
  const size: [number, number] = $derived([view.project.settings?.width ?? 32, view.project.settings?.height ?? 32]);
  // Offset from the opening position once the user drags the card.
  let dx = $state(0);
  let dy = $state(0);

  const drag = (e: PointerEvent) => dragOffset(e, { x: dx, y: dy }, (x, y) => ((dx = x), (dy = y)));

  async function rename(e: Event & { currentTarget: HTMLInputElement }) {
    const to = e.currentTarget.value.trim();
    if (!to || to === id) return;
    const from = id;
    id = to;
    await apply([{ op: "rename_node", id: from, new_id: to }]);
    if (!view.project.nodes?.[to]) id = from;
  }
</script>

{#if node && def}
  <div class="popup" role="dialog" aria-label="{id} settings" style="left: {x + dx}px; top: {y + dy}px; max-height: {Math.max(240, innerHeight - y - dy - 16)}px">
    <div class="bar" role="presentation" onpointerdown={drag}>
      <strong>{def.type}</strong>
      <span class="muted">{def.category}</span>
      <button class="close" aria-label="close" onclick={close}>×</button>
    </div>
    <div class="body">
      <p class="muted desc">{def.description}</p>
      <label class="row"><span class="name">id</span><input value={id} onchange={rename} /></label>
      <label class="row">
        <span class="name">label</span>
        <input value={node.label ?? ""} onchange={(e) => apply([{ op: "set_label", id, label: e.currentTarget.value || null }])} />
      </label>
      {#each def.inputs as input (input.name)}
        {@const raw = node.inputs?.[input.name]}
        {@const ref = parseRef(raw)}
        <div class="row" title={input.description}>
          <span class="name">{input.name}{input.required ? " *" : ""}</span>
          <span class="widget">
            {#if ref}
              <span class="ref">← {ref.node}{ref.output ? "." + ref.output : ""}</span>
              <button onclick={() => apply([resetOp(id, input.name)])}>disconnect</button>
            {:else if input.type === "surface"}
              <span class="muted">connect in the graph</span>
            {:else}
              <Widget def={input} value={raw ?? input.default} {size} change={(v) => apply([literalOp(id, input.name, v)])} />
              {#if raw !== undefined}<button class="icon" title="reset to default" aria-label="reset {input.name}" onclick={() => apply([resetOp(id, input.name)])}>↺</button>{/if}
            {/if}
          </span>
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .popup {
    position: fixed; z-index: 20; width: 360px; display: flex; flex-direction: column;
    background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-md);
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.35);
  }
  .bar { display: flex; gap: 8px; align-items: center; padding: 8px 8px 8px 12px; border-bottom: 1px solid var(--line); cursor: grab; user-select: none; }
  .bar:active { cursor: grabbing; }
  .close { margin-left: auto; width: 28px; height: 28px; padding: 0; border-color: transparent; background: none; }
  .body { overflow: auto; padding: 8px 12px 12px; }
  .desc { margin: 0 0 8px; }
  .row { display: flex; gap: 8px; align-items: center; margin: 6px 0; }
  .name { width: 112px; flex-shrink: 0; font-family: var(--font-mono); font-size: 12px; color: var(--muted); }
  .widget { display: flex; gap: 4px; align-items: center; flex-wrap: wrap; }
  .ref { color: var(--accent); font-family: var(--font-mono); font-size: 12px; }
  .icon { width: 28px; padding: 0; }
</style>
