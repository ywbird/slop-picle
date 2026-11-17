<script lang="ts">
  import { useSvelteFlow } from "@xyflow/svelte";
  import { addOp, type Catalog, type Op } from "./lib/graph";

  let { catalog, add }: { catalog: Catalog; add: (op: Op) => void } = $props();
  const flow = useSvelteFlow();
  let query = $state("");
  let open = $state(false);

  const matches = $derived(
    catalog.filter((d) => `${d.type} ${d.category} ${d.description}`.toLowerCase().includes(query.toLowerCase())),
  );

  function choose(type: string) {
    const r = document.querySelector(".svelte-flow")!.getBoundingClientRect();
    add(addOp(type, flow.screenToFlowPosition({ x: r.left + r.width / 2, y: r.top + r.height / 2 })));
    query = "";
    open = false;
  }
</script>

<div class="add">
  <input
    placeholder="Add node…"
    bind:value={query}
    onfocus={() => (open = true)}
    onkeydown={(e) => {
      if (e.key === "Enter" && matches[0]) choose(matches[0].type);
      if (e.key === "Escape") open = false;
    }}
    aria-label="add node"
  />
  {#if open}
    <ul>
      {#each matches as d (d.type)}
        <li><button onclick={() => choose(d.type)}><strong>{d.type}</strong> <span class="muted">{d.category}</span><br /><small>{d.description}</small></button></li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .add { width: 260px; }
  input { width: 100%; }
  ul { list-style: none; margin: 4px 0 0; padding: 0; max-height: 50vh; overflow: auto; background: var(--panel); border: 1px solid var(--line); border-radius: 4px; }
  li button { width: 100%; text-align: left; border: 0; border-radius: 0; background: none; }
  li button:hover { background: #30343e; }
</style>
