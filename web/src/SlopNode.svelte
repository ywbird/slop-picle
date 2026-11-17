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
  .node { background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-lg); min-width: 180px; max-width: 240px; font-size: 12px; transition: border-color var(--m-out) var(--ease); }
  .node:hover { border-color: var(--muted); }
  .node.selected { border-color: var(--accent); }
  .node.bad { border-color: var(--error); }
  .head { display: flex; justify-content: space-between; align-items: baseline; gap: 8px; padding: 6px 10px; border-bottom: 1px solid var(--line); cursor: pointer; }
  .head strong { font-weight: 500; font-family: var(--font-mono); }
  .head .muted { font-size: 11px; }
  .label { padding: 4px 10px 0; color: var(--muted); }
  .body { display: flex; justify-content: space-between; padding: 6px 0; }
  .row { position: relative; padding: 2px 12px; font-family: var(--font-mono); font-size: 11px; color: var(--muted); }
  .out { text-align: right; }
  .linked { color: var(--accent); }
  .thumb { cursor: zoom-in; display: block; max-width: 64px; max-height: 64px; min-width: 32px; margin: 2px auto 10px; border-radius: 2px; background: repeating-conic-gradient(#232b29 0 25%, #2a3331 0 50%) 0 0 / 8px 8px; }
  .errors { color: var(--error); padding: 6px 10px; border-top: 1px solid var(--line); }
  :global(.h) { width: 9px; height: 9px; border: 2px solid var(--surface); }
  :global(.t-surface) { background: #e0b36a !important; }
  :global(.t-number), :global(.t-int) { background: #8fb7d9 !important; }
  :global(.t-color), :global(.t-palette), :global(.t-gradient) { background: #d48f8a !important; }
  :global(.t-vec2), :global(.t-dimension), :global(.t-padding) { background: #9fc7b8 !important; }
  :global(.t-bool), :global(.t-enum), :global(.t-string), :global(.t-path) { background: #8e948d !important; }
</style>
