// Pure graph logic: project file + catalog → Svelte Flow data, and UI gestures → edit ops (spec/web.md).

export type InputDef = {
  name: string;
  type: string;
  required?: boolean;
  default?: unknown;
  min?: number;
  max?: number;
  options?: string[];
  description: string;
};
export type NodeDef = {
  type: string;
  category: string;
  description: string;
  inputs: InputDef[];
  outputs: { name: string; type: string }[];
};
export type Catalog = NodeDef[];
export type FileNode = { type: string; label?: string; inputs?: Record<string, unknown>; pos?: [number, number] };
export type ProjectFile = {
  picle: 1;
  settings?: { width?: number; height?: number; frames?: number; fps?: number };
  nodes?: Record<string, FileNode>;
};
export type ApiError = { code: string; node?: string; input?: string; op?: number; message: string };
export type Op = Record<string, unknown> & { op: string };
export type XY = { x: number; y: number };

export type NodeData = {
  id: string;
  nodeType: string;
  label?: string;
  def?: NodeDef;
  inputs: Record<string, unknown>;
  errors: string[];
  thumb: string | null;
};
export type FlowNode = { id: string; type: "picle"; position: XY; data: NodeData };
export type FlowEdge = { id: string; source: string; sourceHandle: string; target: string; targetHandle: string };

export function parseRef(v: unknown): { node: string; output: string | null } | null {
  if (!v || typeof v !== "object" || Array.isArray(v)) return null;
  const r = (v as { ref?: unknown }).ref;
  if (typeof r !== "string") return null;
  const i = r.indexOf(".");
  return i < 0 ? { node: r, output: null } : { node: r.slice(0, i), output: r.slice(i + 1) };
}

/** Same rule as the server (spec/types.md#fit-rules); an `any` output's real type is checked by the server. */
export function fits(out: string, input: string): boolean {
  return input === "any" || out === "any" || out === input || (out === "int" && input === "number");
}

export function getDef(catalog: Catalog, type: string): NodeDef | undefined {
  return catalog.find((d) => d.type === type);
}

export function thumbUrl(path: string, node: string, key: string, frame = 0): string {
  return `/api/render?path=${encodeURIComponent(path)}&node=${encodeURIComponent(node)}&frame=${frame}&k=${encodeURIComponent(key)}`;
}

/** One URL per node and key for the session: a key seen at another frame reuses its URL (same picture). */
export function thumbFor(urls: Map<string, string>, path: string, node: string, key: string, frame: number): string {
  const id = `${node}\u0000${key}`;
  let u = urls.get(id);
  if (!u) {
    u = thumbUrl(path, node, key, frame);
    urls.set(id, u);
  }
  return u;
}

/** An animated input value: `{ anim: [{ time, value, ... }], on_end? }`. */
export type AnimKey = { time: number; value: unknown; ease_in_type?: string; ease_out_type?: string };
export function animOf(v: unknown): AnimKey[] | null {
  if (!v || typeof v !== "object" || Array.isArray(v)) return null;
  const a = (v as { anim?: unknown }).anim;
  return Array.isArray(a) ? (a as AnimKey[]) : null;
}

/** Positions for nodes without `pos`: column = longest ref chain to the node, rows in file order. */
export function layout(project: ProjectFile): Record<string, XY> {
  const nodes = project.nodes ?? {};
  const depth: Record<string, number> = {};
  const visiting = new Set<string>();
  const d = (id: string): number => {
    if (id in depth) return depth[id];
    if (visiting.has(id) || !nodes[id]) return 0;
    visiting.add(id);
    let m = -1;
    for (const v of Object.values(nodes[id].inputs ?? {})) {
      const r = parseRef(v);
      if (r && nodes[r.node]) m = Math.max(m, d(r.node));
    }
    visiting.delete(id);
    return (depth[id] = m + 1);
  };
  const rows: Record<number, number> = {};
  const out: Record<string, XY> = {};
  for (const [id, n] of Object.entries(nodes)) {
    const col = d(id);
    const row = rows[col] ?? 0;
    rows[col] = row + 1;
    out[id] = n.pos ? { x: n.pos[0], y: n.pos[1] } : { x: col * 260, y: row * 180 };
  }
  return out;
}

export function toFlow(
  project: ProjectFile,
  catalog: Catalog,
  errors: ApiError[],
  keys: Record<string, string>,
  path: string,
  frame = 0,
  urls?: Map<string, string>,
): { nodes: FlowNode[]; edges: FlowEdge[] } {
  const pos = layout(project);
  const nodes: FlowNode[] = [];
  const edges: FlowEdge[] = [];
  for (const [id, n] of Object.entries(project.nodes ?? {})) {
    const def = getDef(catalog, n.type);
    const surfaceOut = def?.outputs[0]?.type === "surface";
    nodes.push({
      id,
      type: "picle",
      position: pos[id],
      data: {
        id,
        nodeType: n.type,
        label: n.label,
        def,
        inputs: n.inputs ?? {},
        errors: errors.filter((e) => e.node === id).map((e) => (e.input ? `${e.input}: ${e.message}` : e.message)),
        thumb: surfaceOut && keys[id] ? (urls ? thumbFor(urls, path, id, keys[id], frame) : thumbUrl(path, id, keys[id], frame)) : null,
      },
    });
    for (const [input, v] of Object.entries(n.inputs ?? {})) {
      const r = parseRef(v);
      if (!r || !project.nodes?.[r.node]) continue;
      const srcDef = getDef(catalog, project.nodes[r.node].type);
      const output = r.output ?? srcDef?.outputs[0]?.name ?? "out";
      edges.push({ id: `${r.node}.${output}->${id}.${input}`, source: r.node, sourceHandle: output, target: id, targetHandle: input });
    }
  }
  return { nodes, edges };
}

export type Connection = { source: string; sourceHandle?: string | null; target: string; targetHandle?: string | null };

/** The `set_input` op for a new connection, or null when the types do not fit. */
export function connectOp(catalog: Catalog, project: ProjectFile, c: Connection): Op | null {
  const src = project.nodes?.[c.source];
  const dst = project.nodes?.[c.target];
  if (!src || !dst || !c.targetHandle) return null;
  const sdef = getDef(catalog, src.type);
  const ddef = getDef(catalog, dst.type);
  const out = sdef?.outputs.find((o) => o.name === c.sourceHandle) ?? sdef?.outputs[0];
  const input = ddef?.inputs.find((i) => i.name === c.targetHandle);
  if (!out || !input || !fits(out.type, input.type)) return null;
  const ref = out.name === sdef!.outputs[0].name ? c.source : `${c.source}.${out.name}`;
  return { op: "set_input", id: c.target, input: input.name, value: { ref } };
}

export function edgeDeleteOp(e: { target: string; targetHandle?: string | null }): Op {
  return { op: "unset_input", id: e.target, input: e.targetHandle };
}

export function moveOp(id: string, p: XY): Op {
  return { op: "move_node", id, pos: [Math.round(p.x), Math.round(p.y)] };
}

export function addOp(type: string, p: XY): Op {
  return { op: "add_node", type, pos: [Math.round(p.x), Math.round(p.y)] };
}

export function literalOp(id: string, input: string, value: unknown): Op {
  return { op: "set_input", id, input, value };
}

export function resetOp(id: string, input: string): Op {
  return { op: "unset_input", id, input };
}

/** A copy of a node, 40 px down and right; the server picks the id. */
export function duplicateOp(n: FileNode): Op {
  const op: Op = { op: "add_node", type: n.type };
  if (n.label !== undefined) op.label = n.label;
  if (n.inputs && Object.keys(n.inputs).length) op.inputs = JSON.parse(JSON.stringify(n.inputs)); // structuredClone throws on Svelte state proxies
  const [x, y] = n.pos ?? [0, 0];
  op.pos = [Math.round(x) + 40, Math.round(y) + 40];
  return op;
}
