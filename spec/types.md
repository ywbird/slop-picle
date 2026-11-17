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
| `curve` | `{"points": [{"x": 0, "y": 0, "out": [dx, dy]}, {"x": 1, "y": 1, "in": [dx, dy]}], "type": "bezier", "range": [0, 1], "shift": 0, "scale": 1}` | upstream animation curve; 2–16 points, `x` in 0–1, `in`/`out` handle offsets (default `[0, 0]`); `type` `bezier` or `step` (default `bezier`); `range` `[min y, max y]` (default `[0, 1]`); `shift`, `scale` move and stretch `x` (defaults 0, 1; `scale` ≠ 0) |
| `enum` | string | one of the input's `options` |
| `string` | string | |
| `path` | string | relative to the project file's directory; `/` as separator |
| `surface` | — | an image; only reachable through a ref |
| `any` | — | upstream `VALUE_TYPE.any`: refs only; carries whatever value its source gives |

### Fit rules

A literal or a ref output fits an input when the types are equal, with these additions:

- an `int` fits a `number` input;
- a `number` literal with no fraction (e.g. `3.0`) fits an `int` input;
- `enum`, `string` and `path` outputs do not exist yet, so refs to them are not defined;
- an `any` input fits every output; an `any` output has the type of the source of its node's first `any` input. When
  that is unknown (unset, or itself unknown), refs from it are checked at evaluation instead (`type_mismatch` there,
  also for a missing value).
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

### Curve evaluation

Upstream `eval_curve_x` (`scripts/curve_bezier_function`; upstream stores a curve as a flat array, slopc as the object
above). Points are sorted by `x`. For input `x`: `x' = clamp(x / scale − shift, 0, 1)`; `lo, hi = range` (both 0 means
`0, 1`); the result is `lerp(lo, hi, y)` with `y`:

- `bezier`: the first pair of neighbouring points with `x₀ ≤ x' ≤ x₁`. Handles `dx₀, dy₀` = `out` of the first,
  `dx₁, dy₁` = `in` of the second; if `|dx₀| + |dx₁| > 2|x₁ − x₀|` both `dx` scale down by `2|x₁ − x₀| / (|dx₀| + |dx₁|)`.
  `rx = x₁ − x₀`, `t = (x' − x₀)/rx`. All four handle values 0: `y = lerp(y₀, y₁, t)`. Otherwise a cubic Bézier with
  x controls `0, dx₀/rx, 1 + dx₁/rx, 1` and y controls `y₀, y₀ + dy₀, y₁ + dy₁, y₁`: if all y controls are equal,
  that value; else solve `x(u) = t` by Newton's method from `u = t` (8 steps, stop when `|x(u) − t| < 0.0001`), then
  `y = y(u)`. No such pair (the points do not cover `x'`): `y` of the last point (upstream).
- `step`: `y = y` of the first point; for each point but the last, in order: if `x' ≤ point.x` stop, else `y =
  point.y` (so the last point only marks the end, as upstream).

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
| TY-19 | `colorize` of a #808080 `solid` (`t` = 128/255) with keys red at 0 and blue at 1: `rgb`, `constant`, `hsv`, `hsv_inverted`, `oklab`; red to black, `cmyk` | (127,0,128), (255,0,0), (254,0,255), (0,255,2), (139,84,161); (127,64,64); each ± 2 |
| TY-18 | `padding` given `[1, 0, 1, 0]` / `[1, 2]` / `[-1, 0, 0, 0]` | valid / `type_mismatch` / `out_of_range` |
| TY-20 | `curve` given the linear default; one point; 17 points; `type` `"spline"`; `scale` 0; a point without `y` | valid; `out_of_range`; `out_of_range`; `invalid_enum`; `out_of_range`; `type_mismatch` |
| TY-21 | `anim_curve` (`progress` = x) with curve L (linear 0→1, handles ±1/3) at 0.25; D (1→0, handles `[1/3, −1/3]`, `[−1/3, 1/3]`) at 0.25; E (`out [0.6, 0]`, `in [−0.2, −0.2]`) at 0.25 and 0.5; S (`step`, points (0, 0), (0.5, 1), (1, 0.5)) at 0.25, 0.75, 1; L with `range [2, 4]` at 0.5; L with `scale 2` at 0.5; points (0.5, 0.2), (1, 0.8) at 0.25 | 0.25; 0.75; 0.0517, 0.2338; 0, 1, 1; 3; 0.25; 0.8 (each ± 0.0005) |
| TY-22 | `delay_value` with a `color` node as `value`, into `solid.color`; with a `solid` as `value`, into `solid.color`; with `value` unset, into `solid.color` | valid; `type_mismatch`; `missing_input` (a warning) |
| TY-17 | a `color` output (node `color`) into a `solid.color` input; a `number` output into `blend.opacity` | valid |
