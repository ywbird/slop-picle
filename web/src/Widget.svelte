<script lang="ts">
  import type { InputDef } from "./lib/graph";
  import { api, ApiFailure } from "./lib/api";

  let { def, value, size, change }: { def: InputDef; value: unknown; size: [number, number]; change: (v: unknown) => void } = $props();

  const num = (s: string) => (s.trim() === "" ? null : Number(s));
  function setAt<T>(arr: T[], i: number, v: T): T[] {
    const c = [...arr];
    c[i] = v;
    return c;
  }
  const hex6 = (c: string) => (c.length >= 7 ? c.slice(0, 7) : "#000000");
  const alpha = (c: string) => (c.length === 9 ? parseInt(c.slice(7, 9), 16) : 255);
  function withAlpha(c6: string, a: number) {
    const aa = Math.max(0, Math.min(255, Math.round(a)));
    return aa === 255 ? c6 : c6 + aa.toString(16).padStart(2, "0");
  }
  type Grad = { type?: string; keys: { time: number; color: string }[] };
  let uploadError = $state("");

  async function upload(e: Event & { currentTarget: HTMLInputElement }) {
    const f = e.currentTarget.files?.[0];
    if (!f) return;
    const path = `assets/${f.name.replace(/[^A-Za-z0-9._-]/g, "_")}`;
    try {
      await api.upload(path, f);
      uploadError = "";
      change(path);
    } catch (err) {
      uploadError = err instanceof ApiFailure ? err.message : String(err);
    }
  }
</script>

{#if def.type === "number" || def.type === "int"}
  <input
    type="number"
    min={def.min}
    max={def.max}
    step={def.type === "int" ? 1 : "any"}
    value={value as number}
    onchange={(e) => {
      const v = num(e.currentTarget.value);
      if (v !== null && !Number.isNaN(v)) change(v);
    }}
  />
{:else if def.type === "bool"}
  <input type="checkbox" checked={value === true} onchange={(e) => change(e.currentTarget.checked)} />
{:else if def.type === "color"}
  {@const c = String(value)}
  <input type="color" value={hex6(c)} onchange={(e) => change(withAlpha(e.currentTarget.value, alpha(c)))} />
  <input type="number" min="0" max="255" title="alpha" value={alpha(c)} onchange={(e) => change(withAlpha(hex6(c), Number(e.currentTarget.value)))} />
{:else if def.type === "vec2" || def.type === "dimension" || def.type === "padding"}
  {@const n = def.type === "padding" ? 4 : 2}
  {@const arr = (Array.isArray(value) ? value : def.type === "dimension" ? size : Array(n).fill(0)) as number[]}
  {#each Array(n) as _, i (i)}
    <input
      type="number"
      step={def.type === "vec2" ? "any" : 1}
      value={arr[i]}
      onchange={(e) => {
        const v = num(e.currentTarget.value);
        if (v !== null && !Number.isNaN(v)) change(setAt(arr, i, v));
      }}
    />
  {/each}
{:else if def.type === "enum"}
  <select value={value as string} onchange={(e) => change(e.currentTarget.value)}>
    {#each def.options ?? [] as o (o)}<option value={o}>{o}</option>{/each}
  </select>
{:else if def.type === "string" || def.type === "path"}
  <input value={(value as string) ?? ""} onchange={(e) => change(e.currentTarget.value)} />
  {#if def.type === "path"}
    <label class="upload" title="upload a PNG to assets/">⇪<input type="file" accept="image/png" onchange={upload} /></label>
    {#if uploadError}<span class="error">{uploadError}</span>{/if}
  {/if}
{:else if def.type === "palette"}
  {@const cols = (Array.isArray(value) ? value : []) as string[]}
  <div class="list">
    {#each cols as c, i (i)}
      <span class="item">
        <input type="color" value={hex6(c)} onchange={(e) => change(setAt(cols, i, withAlpha(e.currentTarget.value, alpha(c))))} />
        <button title="remove" disabled={cols.length <= 1} onclick={() => change(cols.filter((_, j) => j !== i))}>×</button>
      </span>
    {/each}
    <button onclick={() => change([...cols, cols[cols.length - 1] ?? "#ffffff"])}>+</button>
  </div>
{:else if def.type === "gradient"}
  {@const g = (value ?? { keys: [] }) as Grad}
  <div class="list col">
    <select value={g.type ?? "rgb"} onchange={(e) => change({ ...g, type: e.currentTarget.value })}>
      <option value="rgb">rgb</option>
      <option value="constant">constant</option>
    </select>
    {#each g.keys as k, i (i)}
      <span class="item">
        <input type="number" min="0" max="1" step="0.01" value={k.time} onchange={(e) => change({ ...g, keys: setAt(g.keys, i, { ...k, time: Number(e.currentTarget.value) }) })} />
        <input type="color" value={hex6(k.color)} onchange={(e) => change({ ...g, keys: setAt(g.keys, i, { ...k, color: withAlpha(e.currentTarget.value, alpha(k.color)) }) })} />
        <button title="remove" disabled={g.keys.length <= 1} onclick={() => change({ ...g, keys: g.keys.filter((_, j) => j !== i) })}>×</button>
      </span>
    {/each}
    <button onclick={() => change({ ...g, keys: [...g.keys, { time: 1, color: "#ffffff" }] })}>+ key</button>
  </div>
{:else}
  <span class="muted">{JSON.stringify(value)}</span>
{/if}

<style>
  .list { display: flex; flex-wrap: wrap; gap: 4px; align-items: center; }
  .col { flex-direction: column; align-items: flex-start; }
  .item { display: inline-flex; gap: 2px; }
  input[type="color"] { width: 32px; padding: 0 2px; height: 24px; }
  .upload { cursor: pointer; border: 1px solid var(--line); border-radius: 4px; padding: 1px 6px; }
  .upload input { display: none; }
</style>
