<script lang="ts">
  import { Handle, Position } from "@xyflow/svelte";
  import { parseRef, type NodeData } from "./lib/graph";

  let { data, selected }: { data: NodeData; selected?: boolean } = $props();
  const def = $derived(data.def);
</script>

<div class="node" class:selected class:bad={data.errors.length > 0} title={data.errors.join("\n")}>
  <div class="head">
    <strong>{data.id}</strong>
    <span class="muted">{data.nodeType}</span>
  </div>
  {#if data.label}<div class="label">{data.label}</div>{/if}
  <div class="body">
    <div class="ins">
      {#each def?.inputs ?? [] as input (input.name)}
        <div class="row">
          <Handle type="target" position={Position.Left} id={input.name} class="h t-{input.type}" />
          <span class:linked={parseRef(data.inputs[input.name]) !== null}>{input.name}</span>
        </div>
      {/each}
    </div>
    <div class="outs">
      {#each def?.outputs ?? [] as output (output.name)}
        <div class="row out">
          <span>{output.name}</span>
          <Handle type="source" position={Position.Right} id={output.name} class="h t-{output.type}" />
        </div>
      {/each}
    </div>
  </div>
  {#if data.thumb}<img class="pixel thumb" src={data.thumb} alt="{data.id} preview" draggable="false" />{/if}
  {#if data.errors.length}<div class="errors">{#each data.errors as e (e)}<div>{e}</div>{/each}</div>{/if}
</div>

<style>
  .node { background: var(--panel); border: 1px solid var(--line); border-radius: 6px; min-width: 170px; max-width: 240px; font-size: 12px; }
  .node.selected { border-color: var(--accent); }
  .node.bad { border-color: var(--error); }
  .head { display: flex; justify-content: space-between; gap: 8px; padding: 4px 8px; border-bottom: 1px solid var(--line); }
  .label { padding: 2px 8px; color: var(--muted); font-style: italic; }
  .body { display: flex; justify-content: space-between; padding: 4px 0; }
  .row { position: relative; padding: 1px 10px; }
  .out { text-align: right; }
  .linked { color: var(--accent); }
  .thumb { display: block; max-width: 64px; max-height: 64px; min-width: 32px; margin: 4px auto 8px; background: repeating-conic-gradient(#333 0 25%, #444 0 50%) 0 0 / 8px 8px; }
  .errors { color: var(--error); padding: 4px 8px; border-top: 1px solid var(--line); }
  :global(.h) { width: 8px; height: 8px; }
  :global(.t-surface) { background: #e0a458 !important; }
  :global(.t-number), :global(.t-int) { background: #6fa8dc !important; }
  :global(.t-color), :global(.t-palette), :global(.t-gradient) { background: #c27ba0 !important; }
</style>
