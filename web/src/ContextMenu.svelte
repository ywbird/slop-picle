<script lang="ts">
  import type { Catalog, Op } from "./lib/graph";
  import AddNode from "./AddNode.svelte";

  export type MenuItem = { label: string; run: () => void; danger?: boolean };
  let {
    x,
    y,
    items = [],
    catalog = null,
    add = () => {},
    close,
  }: { x: number; y: number; items?: MenuItem[]; catalog?: Catalog | null; add?: (op: Op) => void; close: () => void } = $props();
</script>

<!-- The editor closes the menu on any click outside it. -->
<div class="menu" role="menu" tabindex="-1" style="left: {x}px; top: {y}px">
  {#if catalog}
    <AddNode {catalog} at={{ x, y }} autofocus add={(op) => (add(op), close())} />
  {:else}
    {#each items as it (it.label)}
      <button role="menuitem" class:danger={it.danger} onclick={() => (it.run(), close())}>{it.label}</button>
    {/each}
  {/if}
</div>

<style>
  .menu { position: fixed; z-index: 30; min-width: 160px; padding: 4px; display: flex; flex-direction: column; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-md); box-shadow: 0 12px 32px rgb(0 0 0 / 0.35); }
  button { text-align: left; border: 0; background: none; padding: 6px 10px; }
  button:hover, button:focus-visible { background: var(--surface-2); transform: none; }
  .danger { color: var(--error); }
</style>
