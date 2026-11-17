<script lang="ts">
  import { api } from "./lib/api";
  import { dragOffset } from "./lib/drag";

  let {
    path,
    id,
    nodeKey,
    frames,
    x,
    y,
    close,
  }: { path: string; id: string; nodeKey: string; frames: number; x: number; y: number; close: () => void } = $props();

  let dx = $state(0);
  let dy = $state(0);
  let zoom = $state(8);
  let frame = $state(0);
  let failed = $state(false);
  const src = $derived(api.renderUrl(path, id, frame, nodeKey));
  const drag = (e: PointerEvent) => dragOffset(e, { x: dx, y: dy }, (a, b) => ((dx = a), (dy = b)));
</script>

<div class="image-popup" role="dialog" aria-label="{id} output" style="left: {x + dx}px; top: {y + dy}px; max-height: {Math.max(240, innerHeight - y - dy - 16)}px">
  <div class="bar" role="presentation" onpointerdown={drag}>
    <strong>{id}</strong>
    <span class="muted">output</span>
    <a class="save" href={src} download="{id}.png">Download PNG</a>
    <button class="close" aria-label="close" onclick={close}>×</button>
  </div>
  <div class="controls">
    <label>zoom <input type="range" min="1" max="32" bind:value={zoom} /></label>
    <label>frame <input type="number" min="0" max={frames - 1} bind:value={frame} /></label>
  </div>
  <div class="canvas">
    <img
      class="pixel"
      {src}
      alt="output of {id}"
      onload={(e) => {
        failed = false;
        const img = e.currentTarget as HTMLImageElement;
        img.style.width = `${img.naturalWidth * zoom}px`;
      }}
      onerror={() => (failed = true)}
      use:size={zoom}
    />
  </div>
  {#if failed}<p class="error">Cannot render this node; see its errors.</p>{/if}
</div>

<script module lang="ts">
  // Keep the shown width at natural width × zoom as either changes.
  function size(img: HTMLImageElement, zoom: number) {
    const apply = (z: number) => {
      if (img.naturalWidth) img.style.width = `${img.naturalWidth * z}px`;
    };
    apply(zoom);
    return { update: apply };
  }
</script>

<style>
  .image-popup {
    position: fixed; z-index: 21; min-width: 240px; max-width: min(720px, calc(100vw - 32px)); display: flex; flex-direction: column;
    background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-md); box-shadow: 0 12px 32px rgb(0 0 0 / 0.35);
  }
  .bar { display: flex; gap: 8px; align-items: center; padding: 8px 8px 8px 12px; border-bottom: 1px solid var(--line); cursor: grab; user-select: none; }
  .bar strong { font-family: var(--font-mono); font-weight: 500; }
  .save { margin-left: auto; font-size: 12px; }
  .close { width: 28px; height: 28px; padding: 0; border-color: transparent; background: none; }
  .controls { display: flex; gap: 16px; align-items: center; padding: 8px 12px; }
  .canvas { overflow: auto; margin: 0 12px 12px; padding: 4px; border-radius: var(--r-sm); background: repeating-conic-gradient(#232b29 0 25%, #2a3331 0 50%) 0 0 / 16px 16px; }
  img { display: block; max-width: none; }
</style>
