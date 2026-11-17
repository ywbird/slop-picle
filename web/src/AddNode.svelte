<script lang="ts">
  import { useSvelteFlow } from "@xyflow/svelte";
  import { addOp, type Catalog, type Op, type XY } from "./lib/graph";

  let {
    catalog,
    add,
    at = null,
    autofocus = false,
  }: { catalog: Catalog; add: (op: Op) => void; at?: XY | null; autofocus?: boolean } = $props();
  const flow = useSvelteFlow();
  let query = $state("");
  let open = $state(false);

  const matches = $derived(
    catalog.filter((d) => `${d.type} ${d.category} ${d.description}`.toLowerCase().includes(query.toLowerCase())),
  );

  function choose(type: string) {
    let point = at;
    if (!point) {
      const r = document.querySelector(".svelte-flow")!.getBoundingClientRect();
      point = { x: r.left + r.width / 2, y: r.top + r.height / 2 };
    }
    add(addOp(type, flow.screenToFlowPosition(point)));
    query = "";
    open = false;
  }

  const focusOn = (el: HTMLInputElement) => {
    if (autofocus) el.focus();
  };
</script>

<div class="add">
  <input
    use:focusOn
    placeholder="Add node…"
    bind:value={query}
    onfocus={() => (open = true)}
    onkeydown={(e) => {
      if (e.key === "Enter" && matches[0]) choose(matches[0].type);
      if (e.key === "Escape") open = false;
    }}
    aria-label="add node"
  />
  {#if open || autofocus}
    <ul>
      {#each matches as d (d.type)}
        <li>
          <button onclick={() => choose(d.type)}>
            <span class="type">{d.type}</span> <span class="muted">{d.category}</span>
            <small class="muted">{d.description}</small>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .add { width: 280px; }
  input { width: 100%; }
  ul { list-style: none; margin: 4px 0 0; padding: 4px; max-height: 48vh; overflow: auto; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-md); }
  li button { width: 100%; text-align: left; border: 0; background: none; display: block; padding: 6px 8px; }
  li button:hover, li button:focus-visible { background: var(--surface-2); transform: none; }
  .type { font-family: var(--font-mono); }
  small { display: block; }
</style>
