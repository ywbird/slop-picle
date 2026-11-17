<script lang="ts">
  // Export dialog (spec/export.md#web-ui).
  import { untrack } from "svelte";
  import { api, ApiFailure, type ProjectView } from "./lib/api";
  import type { Catalog } from "./lib/graph";

  let {
    path,
    view,
    catalog,
    selected,
    frame,
    close,
  }: { path: string; view: ProjectView; catalog: Catalog; selected: string | null; frame: number; close: () => void } = $props();

  const formats = $derived.by(() => {
    const name = path.split("/").pop()!.replace(/\.slopc\.json$/, "");
    return [
      { id: "gif", label: "GIF", out: `exports/${name}.gif` },
      { id: "apng", label: "APNG", out: `exports/${name}.apng` },
      { id: "seq", label: "PNG sequence", out: `exports/${name}_%3f.png` },
      { id: "png", label: "PNG image", out: `exports/${name}.png` },
    ];
  });
  const frames = $derived(view.project.settings?.frames ?? 1);
  const surfaceNodes = $derived(
    Object.entries(view.project.nodes ?? {})
      .filter(([, n]) => catalog.find((d) => d.type === n.type)?.outputs[0]?.type === "surface")
      .map(([id]) => id),
  );

  // Form values start from the project when the dialog opens.
  const start = untrack(() => ({
    node: selected && surfaceNodes.includes(selected) ? selected : (surfaceNodes.find((id) => view.project.nodes?.[id].type === "output") ?? surfaceNodes[0] ?? ""),
    frames,
    fps: view.project.settings?.fps ?? 12,
    out: formats[0].out,
  }));
  let format = $state("gif");
  let out = $state(start.out);
  let node = $state(start.node);
  let first = $state(1);
  let last = $state(start.frames);
  let step = $state(1);
  let scale = $state(1);
  let fps = $state(start.fps);
  let busy = $state(false);
  let error = $state("");
  let files: string[] = $state([]);

  async function run() {
    busy = true;
    error = "";
    files = [];
    const options: Record<string, unknown> = { out, node, scale };
    if (format === "png") options.frame = frame;
    else Object.assign(options, { range: [first, last], step });
    if (format === "gif" || format === "apng") options.fps = fps;
    try {
      files = (await api.export(path, options)).files;
    } catch (e) {
      error = e instanceof ApiFailure ? e.errors.map((x) => x.message).join("\n") : String(e);
    }
    busy = false;
  }
</script>

<div class="export" role="dialog" aria-label="Export">
  <div class="bar">
    <strong>Export</strong>
    <button class="close" aria-label="close" onclick={close}>×</button>
  </div>
  <form
    class="body"
    onsubmit={(e) => {
      e.preventDefault();
      run();
    }}
  >
    <label class="row">
      <span class="name">format</span>
      <select
        value={format}
        onchange={(e) => {
          format = e.currentTarget.value;
          out = formats.find((f) => f.id === format)!.out;
        }}
      >
        {#each formats as f (f.id)}<option value={f.id}>{f.label}</option>{/each}
      </select>
    </label>
    <label class="row">
      <span class="name">node</span>
      <select bind:value={node}>
        {#each surfaceNodes as id (id)}<option value={id}>{id}</option>{/each}
      </select>
    </label>
    {#if format === "png"}
      <div class="row"><span class="name">frame</span><span class="muted">{frame}, as the timeline shows</span></div>
    {:else}
      <div class="row">
        <span class="name">frames</span>
        <input type="number" min="1" max={frames} aria-label="first frame" bind:value={first} />
        <span class="muted">to</span>
        <input type="number" min="1" max={frames} aria-label="last frame" bind:value={last} />
        <span class="muted">of {frames}, from 1</span>
      </div>
      <label class="row"><span class="name">step</span><input type="number" min="1" bind:value={step} /></label>
    {/if}
    {#if format === "gif" || format === "apng"}
      <label class="row"><span class="name">fps</span><input type="number" min="1" max="240" bind:value={fps} /></label>
    {/if}
    <label class="row"><span class="name">scale</span><input type="number" min="1" max="32" bind:value={scale} /></label>
    <label class="row"><span class="name">file</span><input class="out" spellcheck="false" bind:value={out} /></label>
    <button class="primary" disabled={busy || !node}>{busy ? "Exporting…" : "Export"}</button>
    {#if error}<p class="error">{error}</p>{/if}
    {#if files.length}
      <ul class="files">
        {#each files as f (f)}<li><a href={api.fileUrl(f)} download={f.split("/").pop()}>{f}</a></li>{/each}
      </ul>
    {/if}
  </form>
</div>

<style>
  .export {
    position: fixed; z-index: 22; top: 52px; right: 16px; width: 360px; max-height: calc(100vh - 68px); display: flex; flex-direction: column;
    background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-md); box-shadow: 0 12px 32px rgb(0 0 0 / 0.35);
  }
  .bar { display: flex; gap: 8px; align-items: center; padding: 8px 8px 8px 12px; border-bottom: 1px solid var(--line); }
  .bar strong { font-weight: 500; }
  .close { margin-left: auto; width: 28px; height: 28px; padding: 0; border-color: transparent; background: none; }
  .body { overflow: auto; padding: 8px 12px 12px; display: flex; flex-direction: column; }
  .row { display: flex; gap: 8px; align-items: center; margin: 6px 0; }
  .name { width: 64px; flex-shrink: 0; font-family: var(--font-mono); font-size: 12px; color: var(--muted); }
  .row select, .out { flex: 1; min-width: 0; }
  .out { font-family: var(--font-mono); font-size: 12px; }
  .primary { align-self: flex-start; margin-top: 8px; }
  .error { white-space: pre-wrap; margin: 8px 0 0; }
  .files { list-style: none; margin: 8px 0 0; padding: 0; max-height: 160px; overflow: auto; }
  .files a { display: block; padding: 4px 0; font-family: var(--font-mono); font-size: 12px; }
</style>
