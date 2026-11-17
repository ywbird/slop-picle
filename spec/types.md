# Types and catalog

## Value types

| Type | JSON literal | Notes |
|---|---|---|
| `number` | JSON number | 64-bit float; must be finite |
| `int` | JSON number with no fraction | range −2³¹ … 2³¹−1 |
| `bool` | `true` / `false` | |
| `color` | `"#rrggbb"` or `"#rrggbbaa"` | hex digits in either case; `#rrggbb` means alpha `ff`; saved as written |
| `vec2` | `[number, number]` | |
| `dimension` | `[int, int]` | width, height; each 1–4096, product ≤ 2048² (`out_of_range` / `too_large`) |
| `enum` | string | one of the input's `options` |
| `string` | string | |
| `path` | string | relative to the project file's directory; `/` as separator |
| `surface` | — | an image; only reachable through a ref |

### Fit rules

A literal or a ref output fits an input when the types are equal, with these additions:

- an `int` fits a `number` input;
- a `number` literal with no fraction (e.g. `3.0`) fits an `int` input;
- `enum`, `string` and `path` outputs do not exist in M1, so refs to them are not defined yet.

Ranges (`min`, `max`) are inclusive and apply to literals only; refs are checked at evaluation (see [evaluation.md](evaluation.md)).

## Catalog

The catalog is the single description of every node type. Validation, the CLI, MCP and the web UI read it;
nothing about a node type is defined anywhere else.

Shape (also the output of `slopc nodes --json`):

```json
{
  "type": "blend",
  "category": "compose",
  "description": "Draw foreground over background with a blend mode.",
  "inputs": [
    { "name": "background", "type": "surface", "required": true, "description": "..." },
    { "name": "blend_mode", "type": "enum", "options": ["normal", "replace", "multiply", "add", "screen"], "default": "normal", "description": "..." },
    { "name": "opacity", "type": "number", "default": 1, "min": 0, "max": 1, "description": "..." }
  ],
  "outputs": [ { "name": "surface_out", "type": "surface" } ]
}
```

- `required: true` inputs have no `default`. All other inputs have a `default` that fits their type and range,
  except `dimension` inputs, whose `default` is `null`, meaning the project's `width × height`.
- `min` / `max` appear only on `number` and `int` inputs. `options` appears on `enum` inputs, non-empty.
- Every node type has at least one output. Input and output names match `^[a-z][a-z0-9_]*$` and are unique per node.
- Categories in M1: `io`, `generate`, `transform`, `filter`, `color`, `compose`, `value`.
- `type`, input names and output names are stable once released: renaming one breaks saved projects.

## Test cases

| ID | Given | Expected |
|---|---|---|
| TY-01 | `color` literals `#ff0000`, `#FF0000`, `#ff000080` | valid; RGBA (255,0,0,255), (255,0,0,255), (255,0,0,128) |
| TY-02 | `color` literals `#f00`, `red`, `#ff00000`, `#gg0000` | `type_mismatch` each |
| TY-03 | `int` input given `3.0` / `3.5` | valid / `type_mismatch` |
| TY-04 | `number` input given `2` (int) | valid |
| TY-05 | `vec2` given `[1]`, `[1,2,3]`, `["1",2]` | `type_mismatch` each |
| TY-06 | `bool` input given `1` | `type_mismatch` |
| TY-07 | every catalog entry | satisfies the catalog rules above (defaults fit type and range, names unique and well-formed, ≥ 1 output, required ⇔ no default) |
| TY-08 | `slopc nodes --json` | parses; one entry per catalog type; entries equal the shape above |
| TY-09 | a ref from an `int` output to a `number` input | valid |
| TY-10 | a ref from a `number` output to an `int` input | `type_mismatch` |
| TY-11 | `dimension` given `[2.5, 1]`, `[1]` | `type_mismatch` each |
| TY-12 | `dimension` given `[0, 1]`, `[4097, 1]` | `out_of_range` each |
| TY-13 | `dimension` given `[4096, 4096]` | `too_large` |
