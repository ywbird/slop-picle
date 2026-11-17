# Web UI

Svelte 5 + Svelte Flow (`@xyflow/svelte`), built with Vite and bun into `web/dist`, embedded in `slopc serve`.
All rendering happens on the server; the UI shows PNGs from `/api/render`. Layout and naming follow upstream's
panels loosely: graph, node popups, settings.

## Screens

**Login.** Shown when the API answers 401: a token field; submitting calls `POST /api/login`.

**Projects.** The list from `/api/projects`, and a form to create one (path, width, height). Choosing a project opens
the editor; the URL hash holds the path (`#/edit/<path>`), so reloads stay on the project.

**Editor.**

- *Graph* (full width). One generic node component for every type: a header with the id (and label, if any) and the type;
  one row per input with a target handle on the left; output handles on the right; a thumbnail of the first output
  when it is a surface (`/api/render` at scale 1, shown at up to 64 px with pixelated scaling; the URL carries the node's
  key so it reloads only when the key changes). Nodes with errors get a red border and list the messages.
  No Svelte Flow attribution link (user request).
- *Node popup.* Clicking a node opens a floating card beside it (to its right, clamped to the window), draggable by its
  header; an empty-canvas click, Esc or its close button closes it. It holds what the inspector held: id (rename),
  label, and a widget per input type: number and int fields with min/max, bool checkbox, colour picker plus alpha,
  vec2 / dimension / padding as number groups, enum select, string and path text (a `path` input also has an upload
  button: the chosen PNG goes to `assets/<file name>` via `POST /api/upload`, then the input is set to that path),
  palette as a list of colours (add, remove), gradient as type + key list (time, colour). A ref input shows its source
  and a disconnect button. Each input has a reset button (unset to default).
- *Image popup.* Clicking a node's thumbnail opens a floating card beside the node with its first output rendered
  large: pixelated zoom (1–32), frame number (0 … frames − 1) and a link that downloads the PNG. Draggable by its header;
  Esc or its close button closes it. It can be open together with the value popup.
- *Side panel* (right). Project settings only (width, height, frames, fps). Collapsed by default to a thin bar; only its
  toggle opens and closes it (selection does not).
- *Canvas.* A square line grid (24 px cells) with a stronger major line every 5 cells, as in upstream's graph.
  As upstream: left-drag on the empty canvas draws a selection box; middle-drag, or Space + drag, pans; the wheel
  zooms. Dragging any selected node moves the whole selection, saved as one batch of `move_node` ops.
- *Context menus* (right click). On a node: Delete, Duplicate. On an edge: Disconnect. On the empty canvas: the
  add-node search, adding at the cursor. Esc or a click elsewhere closes the menu. Delete/Backspace keep working.
- *Add node.* A search box (top left, and in the canvas menu) listing catalog types by name, category and
  description.

**Look.** From the design-taste-frontend deal for a developer tool (`.taste-skill/history.json`): palette
`green-black-bone` (ground #161c1a, surface #1f2725, ink #e3e2da, muted #8e948d, accent #9fc7b8: a dark, low-glare
ground that leaves the pixel art the brightest thing on screen), type Host Grotesk + JetBrains Mono bundled with the
build (no network needed on a LAN), radius `sharp` (4/6/8). Motion tokens from the skill: hover 150 ms in, 120 ms
out, press 90 ms, ease `cubic-bezier(.2,.8,.2,1)`; buttons step up 1px on hover and sink with scale .985 on press
(fine pointers only); a 2px focus ring in the accent, 2px off the control; one filled button per view. The skill's
marketing-page rules (hero, one section per screen, photographs, page gate) do not apply to an editor.

## Behaviour

Every edit is one call to `POST /api/ops`; the UI never writes files itself.

| Gesture | Op |
|---|---|
| connect output `a.o` to input `b.i` | `set_input {id: b, input: i, value: {ref: "a.o"}}` (`{ref: "a"}` when `o` is the first output) |
| delete an edge | `unset_input` |
| drag a node and drop it | `move_node` with the rounded position |
| delete a node | `remove_node` |
| change a widget | `set_input` with the literal; reset → `unset_input` |
| rename / label | `rename_node` / `set_label` |
| add from search | `add_node {type, pos}` (server picks the id) |
| Duplicate (node menu) | `add_node` with the same type, inputs and label, 40 px down and right (server picks the id) |
| settings | `set_settings` |

- A connection whose output type does not fit the input type ([types.md](types.md#fit-rules)) is refused in the UI
  before any request.
- A rejected batch shows its errors in a toast; the graph stays as the server has it.
- The editor subscribes to `/api/events`; on `changed` it re-reads the project. So edits from MCP or the CLI appear
  without a reload, and thumbnails whose key changed reload.
- Nodes without `pos` are laid out by depth: column = longest ref chain to the node, rows in file order.

## Code layout

- `web/src/lib/graph.ts`: pure functions, no DOM: project + catalog → Svelte Flow nodes and edges, auto layout,
  fit check, gesture → op builders, thumbnail URL. Unit-tested with `bun test`.
- `web/src/lib/api.ts`: fetch wrapper (cookie auth, error shape), EventSource helper.
- `web/src/App.svelte` and components for the screens above.

## Test cases

| ID | Given | Expected |
|---|---|---|
| WEB-01 | `toFlow` on a project with refs | one flow node per project node; one edge per ref, from `node`/output handle to target input handle |
| WEB-02 | `toFlow` with nodes lacking `pos` | positions by depth: sources in column 0, each ref step one column right |
| WEB-03 | `toFlow` with errors | nodes carry their error messages |
| WEB-04 | `connectOp` surface → surface input; first output vs. second output | `set_input` with `{ref: "a"}` / `{ref: "a.out2"}` |
| WEB-05 | `fits` number → int input; int → number; color → surface | false; true; false |
| WEB-06 | `edgeDeleteOp`, `moveOp`, `addOp`, `literalOp`, `resetOp` | the ops in the table above, positions rounded |
| WEB-07 | `thumbUrl` | contains the encoded path, node and key |
| WEB-08 | `bun run build` | succeeds; `web/dist/index.html` exists |
| WEB-09 | headless Chromium opens the served UI with the token, on a project with 3 nodes | the three nodes render with thumbnails; an edit through `slopc edit` adds a node that appears without a reload |
| WEB-10 | headless Chromium: connect, change a number widget (in the popup) | the file on disk has the new ref and value |
| WEB-12 | headless Chromium: drag a node slowly over 30 mouse moves | the node follows the cursor on every move; no project reload during the drag; exactly one `move_node` after the drop (regression: the node flickered back to its old position) |
| WEB-13 | `duplicateOp` on a node with inputs, label and pos | `add_node` with the same type, inputs, label; pos + 40 |
| WEB-14 | headless Chromium: node menu Delete / Duplicate; edge menu Disconnect; canvas menu add at the cursor | node gone from the file / a copy added / input unset / a node near the cursor's flow position |
| WEB-15 | headless Chromium: click a node | a popup beside it with its inputs; Esc closes it |
| WEB-16 | headless Chromium: on load / node selected / toggle twice | panel collapsed / still collapsed / opens with the settings fields, then closes |
| WEB-17 | headless Chromium: the editor | no `.svelte-flow__attribution` element |
| WEB-18 | headless Chromium: click a node's thumbnail; change zoom; Esc | an image card with the node's PNG at its size; the shown width follows the zoom; a download link to `/api/render`; closes |
| WEB-19 | headless Chromium: the canvas | a line-grid background with two pattern layers (minor and major) |
| WEB-20 | headless Chromium: drag a box over two nodes, then drag one of them | both selected; both move; one `/api/ops` request with two `move_node` ops |
| WEB-11 | headless Chromium: upload a PNG on an `image` node's `path` (in the popup) | `assets/<name>.png` exists; the node's `path` is that; its thumbnail shows the image size |
