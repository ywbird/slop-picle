# Spec

One file per area. A spec is written before its code, and changed before the code changes.

| File | Area | Milestone |
|---|---|---|
| [project-file.md](project-file.md) | `*.slopc.json` format, load, save, validation | M1 |
| [types.md](types.md) | value types, node catalog shape | M1 |
| [evaluation.md](evaluation.md) | graph evaluation, cache, GPU conventions | M1 |
| [nodes.md](nodes.md) | node behavior (M1: solid, noise, transform, blend, image, output) | M1, M3 |
| [cli.md](cli.md) | `slopc` commands, output, exit codes | M1, M2 |
| [ops.md](ops.md) | edit operations and batches | M2 |
| [mcp.md](mcp.md) | MCP tools, root sandbox | M2, M5 |
| [server.md](server.md) | `slopc serve`: auth, API, events | M4, M5 |
| [web.md](web.md) | web UI | M4 |
| [animation.md](animation.md) | keyframes, time functions, time remapping, timeline | upstream area 1 |


## Keywords

**must** is a requirement a test checks. **should** is a goal without a test.

## Test cases

Every section ends with a table of test cases:

| ID | Given | Expected |
|---|---|---|
| PF-01 | the input or state | the observable result |

- IDs are `<AREA>-<NN>` and never reused. A removed case stays in the table, struck through.
- Every case has at least one test. The test name contains the ID in lowercase, e.g. `pf_01_minimal_project_loads`.
- Rust tests live in `tests/<area>.rs`. Fixtures live in `tests/fixtures/`, golden images in `tests/golden/`.
- `SLOPC_UPDATE_GOLDEN=1 cargo test` rewrites golden images. A rewritten golden image is reviewed by eye before commit.

Area prefixes: `PF` project file, `TY` types and catalog, `EV` evaluation, `GPU` GPU conventions,
`N-<NODE>` nodes, `CLI` command line, `OP` edit operations, `MCP` MCP server, `SRV` server, `WEB` web UI, `AN` animation.
