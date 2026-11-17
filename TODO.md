# slopc — plan

Node-based procedural pixel-art compositor, built on the ideas of
[Pixel Composer](https://github.com/Ttanasart-pt/Pixel-Composer) (MIT, GameMaker, about 700 nodes).
AI models work through MCP and a CLI. People work through a server with a web UI.
Both edit the same project file.

## Decisions

Confirmed with the user on 2026-11-17.

| Topic | Decision |
|---|---|
| Name | binary `slopc`, project files `*.slopc.json` |
| Core stack | Rust: engine, CLI, MCP and server |
| Rendering | GPU shaders with wgpu + WGSL, headless, on the server only |
| Web UI | Svelte 5 + Svelte Flow (`@xyflow/svelte`); it shows images rendered by the server |
| State | the project file on disk is the source of truth; the server watches it and pushes changes to the web UI |
| MCP transport | stdio, and Streamable HTTP at `/mcp` on the server |
| Deployment | reachable over LAN or remotely, protected by a token |
| Upstream fidelity | ambiguities follow Pixel Composer; deviations are listed in `spec/nodes.md` |
| Random numbers | integer PCG hash, not upstream `fract(sin)`, so golden tests are exact on every GPU |
| Output node | `output` node kept (not upstream); `slopc render -o` picks the path |
| Blend placement | upstream: foreground centred on `position` (default centre); replaces the top-left decision |
| MVP scope | 2D still images, about 30 nodes, PNG output. The evaluator takes a frame number; keyframes and a timeline come later |
| Language | English for docs, code, comments and commits |

## Assumptions

Chosen without asking. Say so if any is wrong.

- No `.pxc` import. Shader math ported from upstream keeps its MIT notice.
- Ambiguities follow upstream behavior first (see `AGENTS.md` rule 1).
- One Rust crate, one binary with subcommands. The web UI is a static Vite build embedded in the binary.
- Surfaces are RGBA8 with straight alpha, sampled nearest-neighbour, no colour management.
- The server speaks plain HTTP. TLS is left to a reverse proxy or a tunnel.
- A token is always required, also on localhost. One is generated when none is given.
- MCP and the server only touch files inside a root directory (default: the current directory).
- Web tooling runs on bun, which is already installed.

## Architecture

```
CLI        MCP (stdio)     MCP (HTTP /mcp)    web UI
 │              │                 │             │ /api, SSE
 │              │                 └─slopc serve─┘
 └──────────────┴───────┬────────────────┘
                      core    project · ops · catalog · validate · eval · gpu
                        │     locked, atomic writes
                  *.slopc.json ── file watch ──► slopc serve ──► web UI
```

- `project`: the file model. Saving takes a file lock and renames atomically.
- `ops`: the one edit vocabulary (`add_node`, `set_input`, ...). The CLI, MCP and the HTTP API all call it. A batch applies fully or not at all.
- `catalog`: every node type with its inputs (type, default, range), outputs and description. The web UI builds its widgets from it, and MCP and the CLI print it, so nothing about a node is written twice.
- `validate`: runs on every load. Errors name the node and the input, so a model can fix its own mistakes.
- `eval`: evaluates the graph for one frame and caches node outputs by content hash.
- `gpu`: headless wgpu. A filter node is one WGSL fragment shader plus its catalog entry.

### Project file

Specified in [spec/project-file.md](spec/project-file.md).

### Nodes for the MVP

33 nodes, named after their upstream nodes (see `spec/nodes.md`).

| Category | Nodes |
|---|---|
| IO | image, output |
| Generate | solid, noise, gradient, shape, cellular, checker, stripe |
| Transform | transform, scale, crop, flip, offset, downscale |
| Filter | blur, outline, shadow, posterize, dither, threshold, displace |
| Colour | color_adjust, invert, color_replace, colorize, greyscale |
| Compose | blend |
| Value | number, color, palette, math, counter |

## Milestones

M3 and M4 do not depend on each other.
Every milestone starts by writing the spec for its areas in `spec/`, then the tests from the spec's cases, then the code.

### M1 — Headless core and CLI

- [x] Spec: project file, types and catalog, evaluation and GPU, M1 nodes, CLI
- [x] Scaffold: Cargo crate `slopc`, `.gitignore`
- [x] Project model: load, save (file lock + atomic rename), `slopc schema` prints the JSON Schema
- [x] Node catalog: types, inputs (type, default, range, enum values), outputs, descriptions
- [x] Validation: unknown type or input, type mismatch, cycle, dangling ref, out-of-range value, caps on surface size and node count
- [x] GPU: headless wgpu device, shader-node helper (uniforms generated from the catalog entry), read back to PNG
- [x] Evaluator: topological order, takes a frame number, caches node outputs by content hash
- [x] First nodes end to end: solid, noise, transform, blend, image, output
- [x] CLI: `new`, `nodes`, `show`, `validate`, `render`
- [x] Golden-image tests with a per-channel tolerance

Done when `slopc render examples/hello.slopc.json -o hello.png` matches its golden image.

### M2 — Editing and MCP over stdio (AI can work)

- [x] Ops: `add_node`, `remove_node`, `rename_node`, `move_node`, `set_input`, `unset_input`, `set_settings`
- [x] CLI: `slopc edit <file> <ops.json | ->`
- [x] `slopc mcp` with six tools: `list_projects`, `create_project`, `read_project`, `edit_project`, `list_node_types`, `render`
- [x] `render` returns the PNG scaled up nearest-neighbour, plus stats (size, opaque bounding box, colour count)
- [x] Root sandbox: every path resolves inside the root, tested against `..` and symlink escapes
- [x] Round-trip test: spawn `slopc mcp`, then create, edit and render

Done when an agent builds a sprite from an empty project using MCP tools only.

### M3 — Node library

- [x] The remaining 27 nodes, each with a catalog description and a golden test

Done when all 33 nodes pass.

### M4 — Server and web UI (people can work)

- [x] `slopc serve --root --host --port --token`: catalog, project list / create / read, ops, render PNG
- [x] Token auth: bearer header for API clients, cookie for the browser. A generated token is printed as a ready-to-open URL
- [x] Checks and limits: `Origin` / `Host` validation, request size cap
- [x] File watch pushed over SSE, with per-node output hashes so only changed thumbnails reload
- [x] Web: graph (Svelte Flow, one generic node component with a thumbnail), add-node search, inspector with a widget per input type, preview (pixelated zoom, frame number), validation errors shown on nodes
- [x] Web build embedded in the binary

Done when a person edits a project in the browser, and an edit made through MCP shows up in the open page without a reload.

### M5 — Remote

- [x] `/mcp` Streamable HTTP in `slopc serve`: same tools, same token
- [x] Upload PNG assets from the web UI

Done when an agent on another machine connects to `/mcp` with the token and renders, and requests without the token are rejected.

## Later

Not in the MVP, in rough order: undo / redo, keyframes and timeline, GIF and sprite-sheet export, groups, loops and feedback, array inputs (batch), custom WGSL node, particles, text and canvas (paint) nodes, simulations, 3D.

## Risks

- GPU output can differ slightly between drivers. Golden tests use a tolerance, and shaders fetch integer texels where they can.
- This machine has an Intel UHD 620 with Vulkan but no software rasteriser. A host without a GPU needs lavapipe.
- A token over plain HTTP can be sniffed on an untrusted network. Use a TLS proxy or a tunnel there.
- A person and an AI can write the same file. Edits are small ops under a file lock, but there is no merge: the last op on an input wins.
- `wgpu` (30.x) and `rmcp` (3.x) change fast. Versions are pinned, and their APIs are checked in M1 and M2.

## Feedback

User feedback as a checklist. Points that need a decision are indented under their item.

- [ ] webui: 노드 선택 delete 키로 삭제
- [ ] webui: 복수 선택 노드 일괄 delete 키 삭제
- [ ] webui: undo 기능 + 단축키
  - [x] Scope: this tab's own edits; undo reverts only what that edit touched, so MCP/CLI edits made since are kept; a conflicting undo is refused with a message; history is lost on reload
  - [x] Redo included. Shortcuts: Ctrl/Cmd+Z undo, Ctrl/Cmd+Shift+Z or Ctrl+Y redo; undo/redo buttons in the top bar

- [x] webui: 패널은 프로젝트 설정만. 노드 눌렀을때 노드 내용은 팝업에만 뜨도록. 노드 출력 이미지는 노드에서 이미지를 눌러서 팝업으로 표시. (commit e95a3dd)
  - [x] Panel: project settings only, collapsed by default, opened and closed only by its button
  - [x] Thumbnail click: a floating card beside the node with zoom, frame and a PNG download link; draggable, Esc closes; may be open with the value popup
- [x] webui: 배경을 그리드 라던가 점 그리드 라던가 뭔가 추가 (commit e95a3dd)
  - [x] Following upstream (Pixel Composer's graph): a square line grid with a stronger major line every 5 cells
- [x] webui: 드래그로 노드 그룹 선택 / 이동 (commit e95a3dd)
  - [x] Following upstream: left-drag on the empty canvas draws a selection box (nodes it touches are selected), middle-drag pans (also Space + drag, scroll zooms); dragging any selected node moves the whole group, saved as one batch

- [x] webui: 노드 삭제가 없는거 같은데 + 우클릭 메뉴 만들어줘. 거기에 삭제 옵션 추가 하는 방식? (commit b4be98f)
  - [x] Node menu: delete, duplicate. Empty canvas menu: add node (search). Edge menu: disconnect
- [x] webui: 우하단에 svelte flow 링크는 없애 (commit b4be98f)
- [x] webui: 노드 눌렀을때 옆에 패널에 값이 나오는 방식 보다는 팝업에서 값 조정. (commit b4be98f)
  - [x] Click a node: a floating card next to it, draggable; closes on empty-canvas click or Esc
- [x] webui: 노드 선택 안했을때 패널은 접혀있도록. (commit b4be98f)
  - [x] Side panel keeps preview + project settings; opens with a selection, collapses without one, has a manual toggle
- [x] webui: design-taste-frontend 스킬 써서 ui 개선. (commit b4be98f)
  - [x] Applied the parts that fit an editor app: dealt palette (green-black-bone), type (Host Grotesk + JetBrains Mono, bundled), radius sharp, motion and focus tokens. The skill's marketing-page rules (hero, one section per screen, photographs, page gate) do not apply; see `spec/web.md` Look

- [x] 노드를 드래그 할때, 드래그 전 위치로 순간이동 했다가, 커서로 순간이동 했다가, 왔다갔다를 반복하고 깜빡거려. (commit 0d41c8b)
  - Cause: the server's file watcher reported reads as changes, so every project read sent a `changed` event and the UI reloaded in a loop, resetting the dragged node each time. Fixed in `src/server.rs`; regression tests SRV-20 and WEB-12.

- [x] Add rule: write the SPEC first, then work
  - [x] Form: `spec/` folder, one file per area
  - [x] Scope now: M1 areas only; other areas when their milestone starts
- [x] Add rule: use test cases actively
  - [x] Each spec section lists numbered test cases; tests are written from them before the code (TDD)
- [x] Write the M1 spec (`spec/`)
  - [x] Generators have an optional `dimension` input (upstream name); absent means the project's size
  - [x] Unknown keys are errors; nodes may have an optional `label` string
  - [x] ~~`blend` puts the foreground at the top-left, with no position input~~ (replaced: follows upstream `position`)
  - [x] `transform` keeps the input size; content moved outside is clipped
  - [x] `slopc new` creates a project with no nodes
  - [x] Limits: 4096 px per side, 2048² px per surface, 1000 nodes
- [x] From now on, resolve ambiguities by following the original project (Pixel Composer) first
  - [x] Re-check the M1 spec and the unasked "Assumptions" against upstream now; keep user-confirmed decisions
  - [x] Noise random: keep the integer hash (deviation from upstream `fract(sin)`)
  - [x] Keep the `output` node (upstream has `Export` instead)
  - [x] Blend: follow upstream `position` (centred), replacing the earlier top-left decision
  - [x] Applied without asking (upstream answers them): upstream input/output names in snake case, `solid` default white, `noise` is white noise with colour modes (value noise moves to a later Perlin-like node), `transform` uses `position`/`anchor` with counter-clockwise rotation and `alpha`, blend modes and formulas ported from `sh_blend_*`, optional `foreground`, `preserve_alpha`, size input named `dimension`
- [x] Ask questions in Korean
- [x] Add rule: when a fix is done, attach the commit id that contains it

