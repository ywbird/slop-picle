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
| `padding` | `[int, int, int, int]` | right, top, left, bottom (upstream order); each ≥ 0 (`out_of_range`) |
| `palette` | `["#rrggbb", ...]` | 1–256 colours (upstream `PALETTE_LIMIT`); empty or longer is `out_of_range` |
| `gradient` | `{"type": "rgb" \| "constant" \| "hsv" \| "hsv_inverted" \| "oklab" \| "cmyk", "keys": [{"time": t, "color": "#rrggbb"}, ...]}` | 1–16 keys, `time` in 0–1; keys are sorted by time when evaluated |
| `enum` | string | one of the input's `options` |
| `string` | string | |
| `path` | string | relative to the project file's directory; `/` as separator |
| `surface` | — | an image; only reachable through a ref |

### Fit rules

A literal or a ref output fits an input when the types are equal, with these additions:

- an `int` fits a `number` input;
- a `number` literal with no fraction (e.g. `3.0`) fits an `int` input;
- `enum`, `string` and `path` outputs do not exist yet, so refs to them are not defined.
- No node outputs `int` yet (M3 `number` outputs `number`), so TY-09 is tested on the fit rule directly.

### Gradient evaluation

Upstream `gradientEval` (`shaders/sh_gradient`, blend types `GRADIENT_INTER` in `scripts/gradients_function`): keys
sorted by `time`; `t` before the first key gives the first colour, after the last key the last colour; a `t` equal to
a key's time gives that key. Between keys `i−1` and `i` (colours `c₀`, `c₁`), with `f = (t − t₀)/(t₁ − t₀)`, alpha
mixes linearly and the colour by `type` (upstream editor names in brackets):

| `type` | Colour |
|---|---|
| `rgb` (RGB) | `mix(c₀, c₁, f)` |
| `constant` (Solid) | `c₀`, alpha included |
| `hsv` (HSV) | `rgb2hsv` both; hue moves the short way: `d = fract(h₁ − h₀)`, `s = fract(2d) − d`, `h = h₀ + s·f`; saturation and value mix linearly; `hsv2rgb` |
| `hsv_inverted` (HSV Inverted) | as `hsv` with `s −= sign(s)` (the long way) |
| `oklab` (OKLAB) | `k = pow(max(0, M·pow(c, 2.2)), 1/3)` for both, mixed, then `pow(max(0, M⁻¹·k³), 1/2.2)`, with upstream's `kCONEtoLMS` / `kLMStoCONE` matrices |
| `cmyk` (CMYK) | `k = 1 − max(r, g, b)`, `(c, m, y) = (1 − rgb − k)/(1 − k)` for both, mixed, then `rgb = (1 − (c, m, y))·(1 − k)`; deviation: black (`k = 1`) gives `(0, 0, 0, 1)` where upstream divides by 0 |

Upstream's `srgb` blend (type 4) is left out: its editor does not offer it.

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
- Every node type has at least one output. Input and output names match `^[a-z][a-z0-9_]*$`; input names are unique
  among inputs and output names among outputs (a value node may reuse its input name for its output, as upstream does).
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
| TY-09 | a ref from an `int` output to a `number` input | valid (no node outputs `int` yet; tested by `fits` unit test) |
| TY-10 | a ref from a `number` output (node `number`) to an `int` input (`noise.seed`) | `type_mismatch` |
| TY-11 | `dimension` given `[2.5, 1]`, `[1]` | `type_mismatch` each |
| TY-12 | `dimension` given `[0, 1]`, `[4097, 1]` | `out_of_range` each |
| TY-13 | `dimension` given `[4096, 4096]` | `too_large` |
| TY-14 | `palette` given `["#000000", "#ffffff"]` / `[]` / `["red"]` / 257 colours | valid / `out_of_range` / `type_mismatch` / `out_of_range` |
| TY-15 | `gradient` given `{"type":"rgb","keys":[{"time":0,"color":"#000000"}]}` | valid |
| TY-16 | `gradient` with `type` `"lab"`, a key `time` 2, no keys, a key missing `color` | `invalid_enum`, `out_of_range`, `out_of_range`, `type_mismatch` |
| TY-17 | `colorize` of a #808080 `solid` (`t` = 128/255) with keys red at 0 and blue at 1: `rgb`, `constant`, `hsv`, `hsv_inverted`, `oklab`; red to black, `cmyk` | (127,0,128), (255,0,0), (254,0,255), (0,255,2), (139,84,161); (127,64,64); each ± 2 |
| TY-18 | `padding` given `[1, 0, 1, 0]` / `[1, 2]` / `[-1, 0, 0, 0]` | valid / `type_mismatch` / `out_of_range` |
| TY-17 | a `color` output (node `color`) into a `solid.color` input; a `number` output into `blend.opacity` | valid |
