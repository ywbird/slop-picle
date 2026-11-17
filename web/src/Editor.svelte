<script lang="ts">
  import { SvelteFlow, Background, Controls, Panel, type Connection } from "@xyflow/svelte";
  import { api, ApiFailure, type ProjectView } from "./lib/api";
  import { connectOp, edgeDeleteOp, moveOp, toFlow, type Catalog, type FlowEdge, type FlowNode, type Op } from "./lib/graph";
  import SlopNode from "./SlopNode.svelte";
  import Inspector from "./Inspector.svelte";
  import AddNode from "./AddNode.svelte";

  let { path, catalog }: { path: string; catalog: Catalog } = $props();

  let view: ProjectView | null = $state(null);
  let nodes: FlowNode[] = $state.raw([]);
  let edges: FlowEdge[] = $state.raw([]);
  let selected: string | null = $state(null);
  let toast = $state("");
  let missing = $state(false);
  const nodeTypes = { slopc: SlopNode };

  async function reload() {
    try {
      view = await api.project(path);
    } catch (e) {
      if (e instanceof ApiFailure && e.status === 404) missing = true;
      return;
    }
    const flow = toFlow(view.project, catalog, view.errors, view.keys, path);
    // Keep unchanged objects so an in-progress drag or connection survives a reload.
    const old = new Map(nodes.map((n) => [n.id, n]));
    const same = (a: FlowNode | undefined, b: FlowNode) =>
      !!a && a.position.x === b.position.x && a.position.y === b.position.y && JSON.stringify(a.data) === JSON.stringify(b.data);
    const next = flow.nodes.map((n) => (same(old.get(n.id), n) ? old.get(n.id)! : { ...n, selected: n.id === selected }));
    if (next.length !== nodes.length || next.some((n, i) => n !== nodes[i])) nodes = next;
    const oldEdges = new Set(edges.map((e) => e.id));
    if (flow.edges.length !== edges.length || flow.edges.some((e) => !oldEdges.has(e.id))) edges = flow.edges;
    if (selected && !view.project.nodes?.[selected]) selected = null;
  }

  async function apply(ops: Op[]) {
    if (ops.length === 0) return;
    try {
      const r = await api.ops(path, ops);
      if (r.created.length === 1) selected = r.created[0];
      toast = "";
    } catch (e) {
      toast = e instanceof ApiFailure ? e.errors.map((x) => `${x.node ? x.node + ": " : ""}${x.message}`).join("\n") : String(e);
    }
    await reload();
  }

  function onconnect(c: Connection) {
    const op = view && connectOp(catalog, view.project, c);
    if (op) apply([op]);
    else toast = "These types do not connect.";
  }

  const isValidConnection = (c: Connection | FlowEdge) => !!view && connectOp(catalog, view.project, c) !== null;

  async function onbeforedelete({ nodes: ns, edges: es }: { nodes: FlowNode[]; edges: FlowEdge[] }) {
    const gone = new Set(ns.map((n) => n.id));
    const ops: Op[] = [
      ...es.filter((e) => !gone.has(e.target)).map(edgeDeleteOp),
      ...ns.map((n) => ({ op: "remove_node", id: n.id })),
    ];
    await apply(ops);
    return false;
  }

  $effect(() => {
    reload();
    const stop = api.events(path, () => reload());
    return stop;
  });
</script>

{#if missing}
  <p class="center">No project <code>{path}</code>. <a href="#/">Back to projects</a></p>
{:else}
  <div class="editor">
    <header>
      <a href="#/">slopc</a>
      <span class="path">{path}</span>
      {#if view}<span class="muted">{view.project.settings?.width ?? 32}×{view.project.settings?.height ?? 32}</span>{/if}
    </header>
    <div class="graph">
      <SvelteFlow
        bind:nodes
        bind:edges
        {nodeTypes}
        fitView
        {onconnect}
        {isValidConnection}
        {onbeforedelete}
        onnodedragstop={({ nodes: moved }) => apply(moved.map((n) => moveOp(n.id, n.position)))}
        onnodeclick={({ node }) => (selected = node.id)}
        onpaneclick={() => (selected = null)}
      >
        <Background />
        <Controls />
        <Panel position="top-left"><AddNode {catalog} add={(op) => apply([op])} /></Panel>
      </SvelteFlow>
      {#if toast}<button class="toast" onclick={() => (toast = "")}>{toast}</button>{/if}
    </div>
    {#if view}
      <Inspector {path} {catalog} {view} bind:selected {apply} />
    {/if}
  </div>
{/if}

<style>
  .editor { display: grid; grid-template-columns: 1fr 340px; grid-template-rows: 36px 1fr; height: 100vh; }
  header { grid-column: 1 / 3; display: flex; gap: 12px; align-items: center; padding: 0 12px; border-bottom: 1px solid var(--line); background: var(--panel); }
  header a { color: var(--accent); font-weight: bold; text-decoration: none; }
  .path { font-family: ui-monospace, monospace; }
  .graph { position: relative; }
  .toast { position: absolute; bottom: 16px; left: 50%; transform: translateX(-50%); white-space: pre-wrap; background: #4a2020; border-color: var(--error); max-width: 70%; text-align: left; z-index: 10; }
  .center { text-align: center; margin-top: 20vh; }
</style>
