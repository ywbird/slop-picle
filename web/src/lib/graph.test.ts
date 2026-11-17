import { describe, expect, test } from "bun:test";
import {
  addOp,
  connectOp,
  edgeDeleteOp,
  fits,
  literalOp,
  moveOp,
  resetOp,
  thumbUrl,
  toFlow,
  type Catalog,
  type ProjectFile,
} from "./graph";

const catalog: Catalog = [
  {
    type: "solid",
    category: "generate",
    description: "",
    inputs: [{ name: "color", type: "color", default: "#ffffff", description: "" }],
    outputs: [{ name: "surface_out", type: "surface" }],
  },
  {
    type: "blend",
    category: "compose",
    description: "",
    inputs: [
      { name: "background", type: "surface", required: true, description: "" },
      { name: "foreground", type: "surface", description: "" },
      { name: "opacity", type: "number", default: 1, min: 0, max: 1, description: "" },
    ],
    outputs: [{ name: "surface_out", type: "surface" }],
  },
  {
    type: "two",
    category: "x",
    description: "",
    inputs: [],
    outputs: [
      { name: "out1", type: "surface" },
      { name: "out2", type: "surface" },
    ],
  },
];

const project: ProjectFile = {
  slopc: 1,
  nodes: {
    a: { type: "solid" },
    b: { type: "solid", pos: [10, 20] },
    m: { type: "blend", inputs: { background: { ref: "a" }, foreground: { ref: "b.surface_out" }, opacity: 0.5 } },
  },
};

describe("toFlow", () => {
  test("WEB-01 nodes and edges", () => {
    const { nodes, edges } = toFlow(project, catalog, [], {}, "p.slopc.json");
    expect(nodes.map((n) => n.id)).toEqual(["a", "b", "m"]);
    expect(edges).toHaveLength(2);
    const bg = edges.find((e) => e.targetHandle === "background")!;
    expect([bg.source, bg.sourceHandle, bg.target]).toEqual(["a", "surface_out", "m"]);
    const fg = edges.find((e) => e.targetHandle === "foreground")!;
    expect([fg.source, fg.sourceHandle]).toEqual(["b", "surface_out"]);
  });

  test("WEB-02 layout by depth, pos kept", () => {
    const { nodes } = toFlow(project, catalog, [], {}, "p.slopc.json");
    const pos = Object.fromEntries(nodes.map((n) => [n.id, n.position]));
    expect(pos.b).toEqual({ x: 10, y: 20 });
    expect(pos.a.x).toBe(0);
    expect(pos.m.x).toBeGreaterThan(pos.a.x);
  });

  test("WEB-03 errors attached", () => {
    const { nodes } = toFlow(project, catalog, [{ code: "x", node: "m", message: "boom" }], {}, "p.slopc.json");
    expect(nodes.find((n) => n.id === "m")!.data.errors).toEqual(["boom"]);
    expect(nodes.find((n) => n.id === "a")!.data.errors).toEqual([]);
  });
});

test("WEB-04 connectOp", () => {
  expect(connectOp(catalog, project, { source: "a", sourceHandle: "surface_out", target: "m", targetHandle: "foreground" })).toEqual({
    op: "set_input",
    id: "m",
    input: "foreground",
    value: { ref: "a" },
  });
  const p2: ProjectFile = { slopc: 1, nodes: { t: { type: "two" }, m: { type: "blend" } } };
  expect(connectOp(catalog, p2, { source: "t", sourceHandle: "out2", target: "m", targetHandle: "background" })!.value).toEqual({
    ref: "t.out2",
  });
  expect(connectOp(catalog, project, { source: "a", sourceHandle: "surface_out", target: "m", targetHandle: "opacity" })).toBeNull();
});

test("WEB-05 fits", () => {
  expect(fits("number", "int")).toBe(false);
  expect(fits("int", "number")).toBe(true);
  expect(fits("color", "surface")).toBe(false);
  expect(fits("surface", "surface")).toBe(true);
});

test("WEB-06 op builders", () => {
  expect(edgeDeleteOp({ target: "m", targetHandle: "background" })).toEqual({ op: "unset_input", id: "m", input: "background" });
  expect(moveOp("a", { x: 10.4, y: -3.6 })).toEqual({ op: "move_node", id: "a", pos: [10, -4] });
  expect(addOp("solid", { x: 1.2, y: 2.7 })).toEqual({ op: "add_node", type: "solid", pos: [1, 3] });
  expect(literalOp("a", "color", "#000000")).toEqual({ op: "set_input", id: "a", input: "color", value: "#000000" });
  expect(resetOp("a", "color")).toEqual({ op: "unset_input", id: "a", input: "color" });
});

test("WEB-07 thumbUrl", () => {
  const u = thumbUrl("art/a b.slopc.json", "n1", "00ff");
  expect(u).toContain("path=art%2Fa%20b.slopc.json");
  expect(u).toContain("node=n1");
  expect(u).toContain("k=00ff");
});
