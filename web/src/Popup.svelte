<script lang="ts">
  import type { ProjectView } from "./lib/api";
  import { animOf, getDef, literalOp, parseRef, resetOp, type Catalog, type Op } from "./lib/graph";
  import Widget from "./Widget.svelte";
  import { dragOffset } from "./lib/drag";

  let {
    catalog,
    view,
    id = $bindable(),
    frame,
    setFrame,
    x,
    y,
    apply,
    close,
  }: {
    catalog: Catalog;
    view: ProjectView;
    id: string;
    frame: number;
    setFrame: (f: number) => void;
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

  const keyOp = (input: string, value: unknown, extra: Record<string, unknown> = {}): Op => ({ op: "set_key", id, input, time: frame, value, ...extra });
  const EASE: Record<string, Record<string, unknown>> = {
    linear: { ease_in_type: "linear", ease_out_type: "linear" },
    bezier: { ease_in_type: "bezier", ease_out_type: "bezier", ease_in: [0.5, 1], ease_out: [0.5, 0] },
    cut: { ease_in_type: "cut", ease_out_type: "cut" },
  };

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
              {@const keys = animOf(raw)}
              {@const here = keys?.find((k) => k.time === frame)}
              {@const at = view.animated[id]?.[input.name]}
              {@const shown = keys ? (typeof at === "number" ? Math.round(at * 1000) / 1000 : at) : (raw ?? input.default)}
              <button
                class="key"
                class:on={!!here}
                class:animated={!!keys}
                aria-label="key {input.name} at frame {frame}"
                title={here ? "Remove the key on this frame" : "Set a key on this frame"}
                onclick={() => apply([here ? { op: "remove_key", id, input: input.name, time: frame } : keyOp(input.name, shown)])}
              >◆</button>
              <Widget
                def={input}
                value={shown}
                {size}
                change={(v) => apply([keys ? keyOp(input.name, v) : literalOp(id, input.name, v)])}
              />
              {#if raw !== undefined}<button class="icon" title="reset to default" aria-label="reset {input.name}" onclick={() => apply([resetOp(id, input.name)])}>↺</button>{/if}
              {#if keys}
                <div class="keys">
                  {#each keys as k (k.time)}
                    <button class="chip" class:current={k.time === frame} onclick={() => setFrame(k.time)}>{k.time}</button>
                  {/each}
                  {#if here}
                    <select
                      aria-label="ease of the key on this frame"
                      value={here.ease_in_type ?? "linear"}
                      onchange={(e) => apply([keyOp(input.name, here.value, EASE[e.currentTarget.value])])}
                    >
                      <option value="linear">linear</option>
                      <option value="bezier">bezier</option>
                      <option value="cut">cut</option>
                    </select>
                  {/if}
                </div>
              {/if}
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
  .key { width: 24px; min-height: 24px; padding: 0; font-size: 10px; color: var(--muted); background: none; border-color: transparent; }
  .key.animated { color: var(--accent); }
  .key.on { color: var(--accent-ink); background: var(--accent); border-color: var(--accent); }
  .keys { display: flex; gap: 4px; flex-wrap: wrap; width: 100%; padding-left: 28px; }
  .chip { min-height: 22px; padding: 0 6px; font-family: var(--font-mono); font-size: 11px; }
  .chip.current { border-color: var(--accent); color: var(--accent); }
</style>
