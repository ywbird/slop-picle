<script lang="ts">
  import type { ProjectView } from "./lib/api";
  import type { Op } from "./lib/graph";

  let { view, apply }: { view: ProjectView; apply: (ops: Op[]) => Promise<void> } = $props();

  let open = $state(false);
  const settings = $derived({ width: 32, height: 32, frames: 1, fps: 12, ...(view.project.settings ?? {}) });
</script>

<aside class="panel" class:collapsed={!open}>
  <button class="toggle" aria-expanded={open} aria-label={open ? "close project settings" : "open project settings"} onclick={() => (open = !open)}>
    {open ? "›" : "‹"}
  </button>
  {#if open}
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
  .panel { position: relative; width: 260px; border-left: 1px solid var(--line); background: var(--surface); overflow: auto; transition: width var(--m-panel) var(--ease); }
  .panel.collapsed { width: 36px; overflow: hidden; }
  .toggle { position: sticky; top: 8px; margin: 8px 0 0 4px; width: 28px; height: 28px; padding: 0; z-index: 1; }
  section { padding: 4px 16px 12px; }
  h3 { margin: 4px 0 8px; font-weight: 500; font-size: 13px; }
  .row { display: flex; gap: 12px; align-items: center; margin: 6px 0; }
  .name { width: 96px; font-family: var(--font-mono); font-size: 12px; color: var(--muted); }
</style>
