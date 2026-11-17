# CLI

One binary, `picle`, with subcommands. M1: `new`, `nodes`, `show`, `validate`, `render`, `schema`. M2: `edit`, `mcp`.
M4: `serve` (see [server.md](server.md)).

## Conventions

- Results go to stdout, diagnostics to stderr.
- `--json` makes stdout machine-readable: exactly one JSON value followed by a newline, also on failure.
- Failure in `--json` mode prints `{ "ok": false, "errors": [ ... ] }` with errors shaped as in
  [project-file.md](project-file.md). Success prints `{ "ok": true, ... }` with the command's fields.

### Exit codes

| Code | Meaning |
|---|---|
| 0 | success |
| 1 | the project is invalid, or evaluation failed |
| 2 | bad command line (unknown flag, missing argument) |
| 3 | I/O error: file missing, not readable, not writable, already exists |
| 4 | `gpu_unavailable` |

## Commands

### `picle new <file> [--width N] [--height N]`

Creates a project with the given settings and no nodes. Fails with exit 3 if the file exists. Missing parent directories
are an error (exit 3), not created. `<file>` must end in `.picle.json` (else exit 2).

### `picle nodes [<type>] [--json]`

Without `<type>`: one line per node type, grouped by category: `type  description`.
With `<type>`: its inputs (name, type, default or `required`, range or options, description) and outputs.
Unknown `<type>` exits 1 and suggests the closest type names.
`--json` prints the catalog array, or the one entry (see [types.md](types.md)).

### `picle show <file> [--json]`

Text: settings, then one line per node: `id  type  input=value ...`, refs printed as `@node` or `@node.output`.
`--json`: the project as loaded, with defaults filled in, plus `"errors"` if any. Exits 1 if invalid, but still prints.

### `picle validate <file> [--json]`

Text: `ok` or one line per error: `<node>.<input>: <code>: <message>`. Exit 0 if valid, 1 if not.

### `picle render <file> -o <png> [--node <id>[.<output>]] [--frame N] [--scale K]`

- Target: `--node`, or else the single `output` node. Zero or several `output` nodes without `--node` → exit 1 with
  error `no_target` / `ambiguous_target`.
- `--frame` defaults to 0. `--scale` is an integer 1–32 (default 1), applied as a nearest-neighbour upscale.
- The target must be a `surface` (else exit 1, `type_mismatch`).
- Writes an 8-bit RGBA PNG atomically. `-o -` writes the PNG to stdout.
- `--json` prints `{ "ok": true, "path": ..., "width": ..., "height": ... }` (size after scaling).

### `picle edit <file> <ops.json | -> [--json]`

Applies a batch of ops ([ops.md](ops.md)) under the project lock. The input is a JSON array of ops, or a single op
object; `-` reads stdin. Exit 0 if applied, 1 if rejected (file unchanged), 3 on I/O errors. `--json` prints the batch
result; text mode prints `ok` (plus warnings) or the errors.

### `picle export <file> -o <out> [options]`

Writes PNG images or sequences, GIFs and APNGs ([export.md](export.md)).

### `picle mcp [--root DIR]`

Runs the MCP server on stdio ([mcp.md](mcp.md)).

### `picle schema`

Prints the JSON Schema (draft 2020-12) of the project file. The schema covers structure only; the catalog rules
(input names per type) are enforced by validation.

## Test cases

| ID | Given | Expected |
|---|---|---|
| CLI-01 | `picle new a.picle.json --width 16 --height 8` | exit 0; file loads with 16×8, no nodes |
| CLI-02 | `picle new` on an existing file | exit 3; file unchanged |
| CLI-03 | `picle new a.json` | exit 2 |
| CLI-04 | `picle nodes` | exit 0; lists all 6 M1 types |
| CLI-05 | `picle nodes blend --json` | exit 0; equals the catalog entry |
| CLI-06 | `picle nodes blnd` | exit 1; stderr suggests `blend` |
| CLI-07 | `picle validate` on a valid file | exit 0; stdout `ok` |
| CLI-08 | `picle validate --json` on the PF-25 file | exit 1; stdout `{"ok":false,"errors":[...3 errors]}` |
| CLI-09 | `picle validate` on a missing file | exit 3 |
| CLI-10 | `picle show` on PF-02 | lists `bg` and `out` with `surface_in=@bg` |
| CLI-11 | `picle render examples/hello.picle.json -o hello.png` | exit 0; PNG equals golden `hello.png` |
| CLI-12 | render a project with two `output` nodes, no `--node` | exit 1; `ambiguous_target` |
| CLI-13 | render a project with no `output` node, `--node bg` | exit 0; PNG of `bg` |
| CLI-14 | `--scale 3` on a 4×4 project | 12×12 PNG; every 3×3 block one colour |
| CLI-15 | `--scale 0` and `--scale 33` | exit 2 |
| CLI-16 | `-o -` | PNG bytes on stdout, nothing else |
| CLI-17 | render with `PICLE_ADAPTER=no-such-gpu` | exit 4 |
| CLI-18 | render an invalid project | exit 1; no output file written |
| CLI-19 | `--json` on every command, success and failure | stdout parses as one JSON value |
| CLI-20 | `picle schema` | valid JSON Schema; validates every valid fixture in `tests/fixtures/`; rejects PF-06 |
| CLI-21 | `picle render <file> -o x.png --frame 1` on a 1-frame project | exit 1; `frame_out_of_range` |
