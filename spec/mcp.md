# MCP server

`slopc mcp [--root DIR]` speaks the Model Context Protocol over stdio (M2). `slopc serve` exposes the same tools over
Streamable HTTP at `/mcp` (M5, see [server.md](server.md)).

## Root sandbox

Every path argument is relative to the root (default: the current directory).

- Absolute paths, and paths that leave the root after resolving `..` and symlinks, fail with `path_outside_root`.
- Project paths must end in `.slopc.json` (`bad_path` otherwise).
- When evaluating, files read by nodes (`image.path`) must also resolve inside the root; otherwise the node fails with
  `eval_error`. The CLI has no such limit.

## Tools

Arguments are JSON objects. Every tool returns text content with one JSON value; `render` adds an image.
A failure sets `isError: true` and the JSON is `{ "ok": false, "errors": [...] }`.

| Tool | Arguments | Result |
|---|---|---|
| `list_projects` | — | `{ "projects": ["a.slopc.json", "sub/b.slopc.json"] }`, sorted; skips hidden directories, `node_modules`, `target` |
| `create_project` | `path`, optional `width`, `height` (default 32) | `{ "ok": true, "path": ... }`; fails with `exists` if the file exists; creates missing parent directories inside the root |
| `read_project` | `path` | `{ "project": <file JSON>, "errors": [...] }` |
| `edit_project` | `path`, `ops` (array) | the batch result from [ops.md](ops.md) |
| `list_node_types` | optional `type` | without: `[{type, category, description}]`; with: the catalog entry |
| `render` | `path`, optional `node`, `frame` (0), `scale` | PNG image + `{ width, height, scale, opaque_bbox, colors }` |

`render`:

- `scale` defaults to the largest integer ≤ 32 that keeps the longer side ≤ 512 (at least 1), so models can see pixels.
- `width`, `height` are the unscaled size. `opaque_bbox` is `[x, y, w, h]` of pixels with alpha > 0, or `null`.
  `colors` is the number of distinct RGBA values.
- Errors as in [cli.md](cli.md#slopc-render): `no_target`, `ambiguous_target`, `eval_error`, `gpu_unavailable`, …

The server's `instructions` tell the model the workflow: `list_node_types` → `create_project` → `edit_project` →
`render`, refs as `{"ref": "node"}`, and that `missing_input` warnings are fine while building.

## Test cases

Tests spawn `slopc mcp --root <tempdir>` and speak JSON-RPC over its stdio.

| ID | Given | Expected |
|---|---|---|
| MCP-01 | `initialize`, then `tools/list` | the six tools, each with an input schema |
| MCP-02 | `create_project a.slopc.json`, then `list_projects` | `["a.slopc.json"]` |
| MCP-03 | `create_project` twice | second fails with `exists`, `isError` |
| MCP-04 | `edit_project` adding solid + output, then `render` | image content (PNG, base64) whose decoded size is scale × project size; stats JSON correct |
| MCP-05 | `render` of an 8×8 project | `scale` 32; image 256×256 |
| MCP-06 | `render` of a 4×4 project with one opaque pixel at (1, 2) on transparent | `opaque_bbox` `[1, 2, 1, 1]`, `colors` 2 |
| MCP-07 | `read_project` | file JSON and `errors` |
| MCP-08 | `edit_project` rejected batch | `isError`; file unchanged |
| MCP-09 | path `../x.slopc.json`, `/tmp/x.slopc.json`, a symlink inside the root to a directory outside | `path_outside_root` each |
| MCP-10 | path `a.json` | `bad_path` |
| MCP-11 | `image` node with path `../secret.png` (outside root), render | `eval_error` |
| MCP-12 | `list_node_types` with and without `type: "blend"` | short list; full blend entry |
| MCP-13 | full round trip: create, edit (noise → transform → output), render | succeeds; image non-empty |
