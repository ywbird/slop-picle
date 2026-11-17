<script lang="ts">
  import { untrack } from "svelte";
  import { SvelteFlow, Background, BackgroundVariant, Controls, Panel, SelectionMode, type Connection } from "@xyflow/svelte";
  import { api, ApiFailure, type ProjectView } from "./lib/api";
  import { connectOp, duplicateOp, edgeDeleteOp, moveOp, toFlow, type Catalog, type FlowEdge, type FlowNode, type Op } from "./lib/graph";
  import SlopNode from "./SlopNode.svelte";
  import Popup from "./Popup.svelte";
  import SidePanel from "./Panel.svelte";
  import ImagePopup from "./ImagePopup.svelte";
  import Timeline from "./Timeline.svelte";
  import AddNode from "./AddNode.svelte";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import Export from "./Export.svelte";

  let { path, catalog }: { path: string; catalog: Catalog } = $props();

  let view: ProjectView | null = $state(null);
  let nodes: FlowNode[] = $state.raw([]);
  let edges: FlowEdge[] = $state.raw([]);
  let selected: string | null = $state(null);
  let frame = $state(0);
  let timeline: Timeline | null = $state(null);
  const frames = $derived((view as ProjectView | null)?.project.settings?.frames ?? 1);

  function setFrame(f: number) {
    return showFrame(Math.max(0, Math.min(frames - 1, f)));
  }
  let toast = $state("");
  let missing = $state(false);
  const nodeTypes = { slopc: SlopNode };
  let popup: { x: number; y: number } | null = $state(null);
  let imagePopup: { id: string; x: number; y: number } | null = $state(null);
  let menu: { x: number; y: number; items?: MenuItem[]; add?: boolean } | null = $state(null);
  let exporting = $state(false);

  /** Where a card opens beside node `id`: right of it, or left when there is no room. */
  function beside(id: string, w: number) {
    const r = document.querySelector(`.svelte-flow__node[data-id="${CSS.escape(id)}"]`)?.getBoundingClientRect();
    if (!r) return null;
    const x = r.right + 12 + w > innerWidth ? Math.max(8, r.left - w - 12) : r.right + 12;
    return { x, y: Math.max(48, Math.min(r.top, innerHeight - 320)) };
  }

  function nodeClick(id: string, e: MouseEvent | TouchEvent) {
    if ((e.target as Element).closest("img.thumb")) {
      const at = beside(id, 360);
      if (at) imagePopup = { id, ...at };
      return;
    }
    selected = id;
    popup = beside(id, 360);
  }

  function closeAll() {
    selected = null;
    popup = null;
    imagePopup = null;
    menu = null;
  }

  function nodeMenu(id: string, e: MouseEvent) {
    e.preventDefault();
    const n = view?.project.nodes?.[id];
    if (!n) return;
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: "Duplicate", run: () => apply([duplicateOp(n)]) },
        { label: "Delete", danger: true, run: () => apply([{ op: "remove_node", id }]) },
      ],
    };
  }

  // Thumbnail URLs per node and key, and preloaded images kept alive so the browser can show them at once.
  const urls = new Map<string, string>();
  const preloaded = new Map<string, HTMLImageElement>();
  let seq = 0;

  function preload(list: string[]) {
    return Promise.all(
      list.map((u) => {
        let img = preloaded.get(u);
        if (!img) {
          img = new Image();
          img.src = u;
          preloaded.set(u, img);
          // shortcut: keeps the newest 4000 images; enough for long loops of small sprites, raise for larger ones.
          if (preloaded.size > 4000) preloaded.delete(preloaded.keys().next().value!);
        }
        return img.decode().catch(() => {});
      }),
    );
  }

  /** Load frame `f` and show it only once its data and changed images are ready; a newer call supersedes this one. */
  async function showFrame(f: number): Promise<boolean> {
    const mine = ++seq;
    let v: ProjectView;
    try {
      v = await api.project(path, f);
    } catch (e) {
      if (e instanceof ApiFailure && e.status === 404) missing = true;
      return false;
    }
    if (mine !== seq) return false;
    if (f >= (v.project.settings?.frames ?? 1)) return showFrame(0);
    const flow = toFlow(v.project, catalog, v.errors, v.keys, path, f, urls);
    const shown = new Set(nodes.map((n) => n.data.thumb));
    await preload(flow.nodes.map((n) => n.data.thumb).filter((u): u is string => !!u && !shown.has(u)));
    if (mine !== seq) return false;
    view = v;
    frame = f;
    // Keep unchanged objects so an in-progress drag or connection survives a reload.
    const old = new Map(nodes.map((n) => [n.id, n]));
    const same = (a: FlowNode | undefined, b: FlowNode) =>
      !!a && a.position.x === b.position.x && a.position.y === b.position.y && JSON.stringify(a.data) === JSON.stringify(b.data);
    const next = flow.nodes.map((n) => (same(old.get(n.id), n) ? old.get(n.id)! : { ...n, selected: n.id === selected }));
    if (next.length !== nodes.length || next.some((n, i) => n !== nodes[i])) nodes = next;
    const oldEdges = new Set(edges.map((e) => e.id));
    if (flow.edges.length !== edges.length || flow.edges.some((e) => !oldEdges.has(e.id))) edges = flow.edges;
    if (selected && !v.project.nodes?.[selected]) selected = null;
    return true;
  }

  const reload = () => showFrame(frame);

  type Step = { undo: Op[]; redo: Op[] };
  let undoStack: Step[] = $state([]);
  let redoStack: Step[] = $state([]);

  function showErrors(e: unknown) {
    toast = e instanceof ApiFailure ? e.errors.map((x) => `${x.node ? x.node + ": " : ""}${x.message}`).join("\n") : String(e);
  }

  async function apply(ops: Op[]) {
    if (ops.length === 0) return;
    try {
      const r = await api.ops(path, ops);
      if (r.created.length === 1) selected = r.created[0];
      if (r.undo.length) {
        undoStack = [...undoStack.slice(-99), { undo: r.undo, redo: r.redo }];
        redoStack = [];
      }
      toast = "";
    } catch (e) {
      showErrors(e);
    }
    await reload();
  }

  /** Move the last step from one stack to the other, sending its undo or redo ops. */
  async function travel(back: boolean) {
    const from = back ? undoStack : redoStack;
    const step = from[from.length - 1];
    if (!step) return;
    if (back) undoStack = undoStack.slice(0, -1);
    else redoStack = redoStack.slice(0, -1);
    try {
      await api.ops(path, back ? step.undo : step.redo);
      if (back) redoStack = [...redoStack, step];
      else undoStack = [...undoStack, step];
      toast = "";
    } catch (e) {
      showErrors(e);
    }
    await reload();
  }

  function shortcut(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    if (t.closest("input, textarea, select, [contenteditable]")) return;
    if (frames > 1 && !(e.ctrlKey || e.metaKey)) {
      if (e.key === ",") return setFrame((frame - 1 + frames) % frames);
      if (e.key === ".") return setFrame((frame + 1) % frames);
      if (e.key === " " && !t.closest("button")) {
        e.preventDefault();
        return timeline?.toggle();
      }
    }
    if (!(e.ctrlKey || e.metaKey)) return;
    const k = e.key.toLowerCase();
    if (k === "z" && !e.shiftKey) travel(true);
    else if ((k === "z" && e.shiftKey) || (k === "y" && !e.metaKey)) travel(false);
    else return;
    e.preventDefault();
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

  // Depend on `path` only: reload() reads `frame`, and re-running this on every frame change reopened the events
  // stream and raced playback.
  $effect(() => {
    const p = path;
    return untrack(() => {
      reload();
      return api.events(p, () => reload());
    });
  });
</script>

<svelte:window
  onkeydown={(e) => {
    shortcut(e);
    if (e.key === "Escape") {
      if (menu) menu = null;
      else if (imagePopup) imagePopup = null;
      else if (exporting) exporting = false;
      else popup = null;
    }
  }}
  onpointerdown={(e) => {
    if (!(e.target as Element).closest(".menu")) menu = null;
  }}
/>

{#if missing}
  <p class="center">No project <code>{path}</code>. <a href="#/">Back to projects</a></p>
{:else}
  <div class="editor">
    <header>
      <a href="#/">slopc</a>
      <span class="path">{path}</span>
      {#if view}<span class="muted">{view.project.settings?.width ?? 32}×{view.project.settings?.height ?? 32}</span>{/if}
      <button class="export-open" aria-expanded={exporting} disabled={!view} onclick={() => (exporting = !exporting)}>Export</button>
      <span class="history">
        <button aria-label="undo" title="Undo (Ctrl+Z)" disabled={undoStack.length === 0} onclick={() => travel(true)}>↶</button>
        <button aria-label="redo" title="Redo (Ctrl+Shift+Z)" disabled={redoStack.length === 0} onclick={() => travel(false)}>↷</button>
      </span>
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
        proOptions={{ hideAttribution: true }}
        deleteKey={["Delete", "Backspace"]}
        panActivationKey={null}
        onnodedragstop={({ nodes: moved }) => apply(moved.map((n) => moveOp(n.id, n.position)))}
        onnodeclick={({ node, event }) => nodeClick(node.id, event)}
        selectionOnDrag
        selectionMode={SelectionMode.Partial}
        panOnDrag={[1]}
        onpaneclick={closeAll}
        onnodecontextmenu={({ node, event }) => nodeMenu(node.id, event)}
        onedgecontextmenu={({ edge, event }) => {
          event.preventDefault();
          menu = { x: event.clientX, y: event.clientY, items: [{ label: "Disconnect", run: () => apply([edgeDeleteOp(edge as FlowEdge)]) }] };
        }}
        onpanecontextmenu={({ event }) => {
          event.preventDefault();
          menu = { x: event.clientX, y: event.clientY, add: true };
        }}
      >
        <Background id="minor" variant={BackgroundVariant.Lines} gap={24} lineWidth={1} patternColor="#222b28" />
        <Background id="major" variant={BackgroundVariant.Lines} gap={120} lineWidth={1} patternColor="#2e3a36" />
        <Controls />
        <Panel position="top-left"><AddNode {catalog} add={(op) => apply([op])} /></Panel>
        {#if menu}
          <ContextMenu x={menu.x} y={menu.y} items={menu.items} catalog={menu.add ? catalog : null} add={(op) => apply([op])} close={() => (menu = null)} />
        {/if}
      </SvelteFlow>
      {#if toast}<button class="toast" onclick={() => (toast = "")}>{toast}</button>{/if}
    </div>
    {#if frames > 1 && view}
      <div class="bottom"><Timeline bind:this={timeline} {frame} {frames} fps={view.project.settings?.fps ?? 12} {setFrame} /></div>
    {/if}
    {#if view}
      <SidePanel {view} {apply} />
      {#if popup && selected}
        <Popup {catalog} {view} {frame} {setFrame} bind:id={selected} x={popup.x} y={popup.y} {apply} close={() => (popup = null)} />
      {/if}
      {#if exporting}
        <Export {path} {view} {catalog} {selected} {frame} close={() => (exporting = false)} />
      {/if}
      {#if imagePopup && view.project.nodes?.[imagePopup.id]}
        <ImagePopup
          id={imagePopup.id}
          src={nodes.find((n) => n.id === imagePopup?.id)?.data.thumb ?? ""}
          {frame}
          x={imagePopup.x}
          y={imagePopup.y}
          close={() => (imagePopup = null)}
        />
      {/if}
    {/if}
  </div>
{/if}

<style>
  .editor { display: grid; grid-template-columns: 1fr auto; grid-template-rows: 44px 1fr auto; height: 100vh; }
  .bottom { grid-column: 1 / 3; grid-row: 3; }
  .editor > :global(aside.panel) { grid-column: 2; grid-row: 2; }
  header { grid-column: 1 / 3; display: flex; gap: 16px; align-items: center; padding: 0 16px; border-bottom: 1px solid var(--line); background: var(--surface); }
  header a { color: var(--ink); font-weight: 500; text-decoration: none; letter-spacing: -0.01em; }
  .path { font-family: var(--font-mono); font-size: 12px; color: var(--muted); }
  .export-open { margin-left: auto; }
  .history { display: flex; gap: 4px; }
  .history button { width: 32px; padding: 0; }
  .graph { position: relative; min-width: 0; }
  .toast { position: absolute; bottom: 16px; left: 50%; transform: translateX(-50%); white-space: pre-wrap; background: #3a2422; border-color: var(--error); max-width: 70%; text-align: left; z-index: 10; }
  .center { text-align: center; margin-top: 20vh; }
</style>
