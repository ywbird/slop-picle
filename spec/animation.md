# Animation

Upstream: Pixel Composer `scripts/node_keyframe` (`valueKey`, `valueAnimator`, `KeyframeInterpolate`),
`scripts/curve_bezier_function`, `scripts/node_value` (`lerpAnimKeys`), Animation category nodes.

A project has `settings.frames` (count) and `settings.fps`. Evaluation already takes a frame number
([evaluation.md](evaluation.md)). This spec adds keyframed inputs, time functions and time-remapping nodes, and the
API/UI parts to edit and play animation.

## Keyframed inputs

Any input whose type is `number`, `int`, `bool`, `color`, `vec2`, `dimension`, `padding`, `enum`, `string`, `path`,
`palette` or `gradient` may hold an animation instead of a literal or a ref (`surface` inputs only take refs):

```json
"rotation": {
  "anim": [
    { "time": 0, "value": 0 },
    { "time": 12, "value": 90, "ease_in_type": "bezier", "ease_in": [0.5, 1] }
  ],
  "on_end": "hold"
}
```

| Key field | Type | Default | Rule |
|---|---|---|---|
| `time` | int | required | frame, ≥ 0; unique within the animation |
| `value` | the input's literal | required | checked like a literal (type, range, options) |
| `ease_in`, `ease_out` | `[number, number]` | `[0, 1]`, `[0, 0]` | bezier handle (x, y) of the incoming / outgoing side |
| `ease_in_type`, `ease_out_type` | enum `linear`, `bezier`, `cut` | `linear` | |

`on_end` is `hold` (default), `loop`, `ping` or `wrap` (upstream `KEYFRAME_END`). The animation object has exactly the
keys `anim` and optionally `on_end`; keys are evaluated sorted by `time` (they are saved as written). At least one key.

Validation errors carry the `node` and `input`: `type_mismatch` (a malformed key, a bad value type, an animation on a
surface input), `out_of_range` (no keys, negative or duplicate `time`, a value out of range), `invalid_enum`
(`on_end`, ease types, enum values), `unknown_key`.

### Value at frame `t`

With keys `k₀ … kₙ₋₁` sorted, `first = k₀.time`, `last = kₙ₋₁.time`, `T = settings.frames`:

1. One key: its value.
2. `t > last`: `loop` → `t = first + (t − last) mod (last − first + 1)`; `ping` → `p = (t − first) mod 2(last − first)`,
   `t = first + p` if `p < last − first`, else `first + 2(last − first) − p`. (`hold`, `wrap`: unchanged.)
3. `t < first`: `wrap` interpolates from `kₙ₋₁` to `k₀` with `rat = (T − last + t)/(T − last + first)`; otherwise
   `k₀.value`.
4. `t ≥ last` (after step 2): `wrap` interpolates from `kₙ₋₁` to `k₀` with `rat = (t − last)/(T − last + first)`;
   otherwise `kₙ₋₁.value`.
5. Otherwise between `from = kᵢ ≤ t < to = kᵢ₊₁`: `rat = (t − from.time)/(to.time − from.time)`.

Interpolating `from → to` at `rat`: if `to.ease_in_type` is `cut` → `from.value`; else if `from.ease_out_type` is
`cut` → `to.value`; else `e = ease(from, to, rat)` and the value is `lerp(from.value, to.value, e)` per component
(numbers, ints, vec2, dimension, padding, colour channels in 0–255 incl. alpha). Ints, dimensions, paddings and colour
channels round to the nearest integer. Types that cannot be interpolated (`bool`, `enum`, `string`, `path`, `palette`,
`gradient`) give `from.value` until `to.time`.

`ease(from, to, rat)` (upstream `KeyframeInterpolate`): if both `to.ease_in_type` and `from.ease_out_type` are
`linear` → `rat`. If `rat` is 0 or 1 → `rat`. Otherwise with `(eox, eoy) = from.ease_out`, `(eix, eiy) = to.ease_in`:
if `eox > 1 and eix > 1`, divide both by `min(eox, eix)`; `eiy −= clamp(eox − 1, 0, 1)`; `eox = clamp(eox, 0, 0.9)`;
`eoy += clamp(eix − 1, 0, 1)`; `eix = clamp(eix, 0, 0.9)`; result = the cubic bezier `y` at `x = rat` for control
points `(0, 0), (eox, eoy), (1 − eix, eiy), (1, 1)`, solved for `t` by up to 8 Newton steps from `t = rat`
(stop when `|x(t) − rat| < 0.0001`), as upstream `eval_curve_segment_x`.

A node with an animated input depends on the frame (its cache key includes the frame); other nodes do not.

### Ops

Animations can be written whole with `set_input`. Two ops edit single keys ([ops.md](ops.md)):

| `op` | Fields | Effect |
|---|---|---|
| `set_key` | `id`, `input`, `time`, `value`, optional `ease_in`, `ease_out`, `ease_in_type`, `ease_out_type` | Adds or replaces the key at `time`. On a literal or unset input it starts an animation with just this key. On a ref input: `bad_op`. |
| `remove_key` | `id`, `input`, `time` | Removes that key (`bad_op` if none). Removing the last key leaves its value as a plain literal. |

## Time functions (value nodes)

| Type (upstream) | Inputs | Output `output` (number) |
|---|---|---|
| `wiggler` (`Node_Wiggler`) | `seed` int 0; `range` vec2 `[0, 1]`; `frequency` number 4, 1–16; `clip` int 3, 0–3 (bit 1: start, bit 2: end) | see below; reads the frame |
| `fn_ease` (`Node_Fn_Ease`) | `range` vec2 `[0, 1]`; `amount` vec2 `[0.1, 0.9]`; `smooth` enum `cubic_poly`, `quadratic_rat`, `cubic_rat`, `cosine` | `x = frame / T`; 0 outside `range`; else `s(clamp(min(a₀ = 0 ? 1 : (x − r₀)/a₀, b = 0 ? 1 : 1 − (x − (r₁ − b))/b), 0, 1))` with `b = 1 − a₁`; reads the frame |
| `fn_smoothstep` (`Node_Fn_SmoothStep`) | `value` number 0; `type` enum as `smooth` | `s(value)` |
| `fn_math` (`Node_Fn_Math`) | `operation` enum `add`, `minus`, `multiply` (default `multiply`); `value_1`, `value_2` numbers 0 | the operation |

Smooth functions `s(x)`: `cubic_poly` `x²(3 − 2x)`; `quadratic_rat` `x²/(2x² − 2x + 1)`; `cubic_rat`
`x³/(3x² − 3x + 1)`; `cosine` `0.5 − 0.5 cos(πx)`.

Wiggler at frame `f`: `w = frequency = 0 ? 1 : max(1, T / frequency)`; `lo = floor(f / w)·w`, `hi = min(T, lo + w)`;
`v₀ = (clip & 1 and lo ≤ 0) ? 0.5 : r(lo)`, `v₁ = (clip & 2 and hi ≥ T) ? 0.5 : r(hi)` with
`r(x) = unit(hash2(round(x·1000), 0, seed))` (deviation: integer hash instead of upstream `random1D` and without the
project seed); `t = (f − lo)/(hi − lo)`, `t = (1 − cos πt)/2`; output `lerp(range₀, range₁, lerp(v₀, v₁, t))`.

## Time remapping (surface nodes)

These evaluate their `surface` input at another frame `s` of the same graph (upstream caches frames instead; slopc
recomputes them, which gives the same pictures). When `s` is "empty", the output is a transparent surface of the
input's size at the current frame. All read the frame. `T = settings.frames`, `f` the frame.

| Type (upstream) | Inputs (output name) | Source frame `s` |
|---|---|---|
| `revert` (`Node_Revert`) | `surface_in` required (`output`) | `T − 1 − f` |
| `anim_speed` (`Node_Anim_Speed`) | `surface_in` required; `speed` number 1; `overflow` enum `loop`, `pingpong`, `empty` (`surface_out`) | `x = f·speed`; `loop`: `x mod T`; `pingpong`: `x mod (2T + 1)`, then `2T − x` if `x > T`; `empty`: empty if `x < 0` or `x ≥ T`. `s = floor(x)`, clamped to `0 … T − 1` |
| `rate_remap` (`Node_Rate_Remap`) | `surface` required; `framerate` number 10, ≥ 1; `offset` number 0; `loop` bool true (`surface`) | `x = f − offset`, `loop` → `x mod T` (non-negative); `step = fps / framerate`; `s = floor(floor(x / step)·step)` |
| `delay` (`Node_Delay`) | `surface` required; `frames` int 1; `overflow` enum `hold`, `loop`, `clear` (`surface`) | `x = f − frames`; `hold`: clamp to `0 … T − 1`; `loop`: `(x mod T)` non-negative; `clear`: empty outside `0 … T − 1` |
| `anim_loop` (`Node_Anim_Loop`) | `surface` required; `loop_type` enum `loop`, `pingpong`; `loop_start` int 1; `loop_range` int 4; `infinite` bool true; `loop_amount` int 1; `pre_loop`, `post_loop` enum `passthrough`, `empty` (defaults `passthrough`, `empty`) (`surface`) | `start = loop_start − 1`, `range = loop_range = 0 ? T − loop_start : loop_range`; `f < start`: pre-loop (`f` or empty); `n = floor((f − start)/range)`; `n > loop_amount` and not `infinite`: post-loop (`f` or empty); else `p = (f − start) mod range`, `s = start + p` for `loop`, and for `pingpong` `start + p` on even `n`, `start + range − 1 − p` on odd `n` |

Left out of this area: `Node_Anim_Curve` and `Node_Stagger` (they need a curve input type), `Node_Delay_Value`
(an "any" type), audio, optical flow, keyframe drivers, `Node_Timeline_Preview`. They move to the node-completion area.

## Server

`GET /api/project` and `GET /api/events` take an optional `frame` (default 0): `keys` are computed for that frame, and
the response adds `animated`: `{ node: { input: value at frame } }` for every animated input. The events stream keeps
using the frame given when it was opened. `GET /api/render` already takes `frame`.

## Web UI

- *Timeline* (bottom bar, shown when `frames > 1`): first/previous/play-pause/next buttons, a frame slider
  `0 … frames − 1` and the frame number. Play advances one frame per `1/fps` s and loops. The current frame drives
  thumbnails, image popups and the values shown in node popups.
- *Keys in the node popup*: every animatable input has a key button. Not animated → it sets a key at the current
  frame with the current value. Animated → the widget shows the value at the current frame; a filled key button means a
  key sits on this frame (click: `remove_key`), a hollow one means none (click: `set_key`). Changing the widget of an
  animated input sets a key at the current frame. An "ease" select for the key on this frame sets both its ease types
  (`bezier` also sets `ease_in [0.5, 1]` and `ease_out [0.5, 0]`, an ease-in-out; deviation: upstream's default handles
  differ per preference). The popup lists the input's key frames; clicking one jumps there.
- Keyboard: `,` and `.` step frames, Space plays/pauses (outside fields).

## MCP

The tool instructions mention keyframes (`set_key`, the `anim` form) and that `render` takes `frame`.

## Test cases

| ID | Given | Expected |
|---|---|---|
| AN-01 | an animated `transform.rotation` as above | valid |
| AN-02 | key value `"x"` on a number input; animation on `transform.surface_in` | `type_mismatch` with node and input |
| AN-03 | `anim: []`; two keys at time 3; a key at time −1; value 2 on `blend.opacity` | `out_of_range` each |
| AN-04 | `on_end: "bounce"`; `ease_in_type: "smooth"` | `invalid_enum` each |
| AN-05 | keys (0, 0) and (10, 100), linear, frames 20: values at 0, 5, 10, 15 | 0, 50, 100, 100 |
| AN-06 | keys (5, 1) and (8, 2): frame 0; one key (3, 7): any frame | 1; 7 |
| AN-07 | keys (0, 0), (10, 100) with the second key's `ease_in_type: cut`; then instead the first key's `ease_out_type: cut` | frame 5 gives 0; then 100 |
| AN-08 | bezier ease-out `[0.5, 0]` / ease-in `[0.5, 1]` between 0 and 1 | `e(0.5) = 0.5` (± 1e-4); `e(0.25) < 0.25`; `e(0.25) + e(0.75) = 1` (± 1e-4); monotonic over 0, 0.1 … 1 |
| AN-09 | keys (2, 0), (6, 40), frames 20: `loop` at 7, 11; `ping` at 8, 10; `wrap` at 13 and 1 | 10, 0; 20, 0; 22.5, 2.5 |
| AN-10 | int input keys 0 → 3 at frame 1 of 0..2; vec2 `[0, 0] → [10, 20]` at half; colour `#000000 → #ff0000` at half; enum `normal → add` at half | 2 (1.5 rounds up); `[5, 10]`; `(128, 0, 0, 255)`; `normal` |
| AN-11 | a solid with an animated colour feeding a transform, and a separate static solid: evaluate frames 0 then 1 | frame 1 re-runs the animated solid and the transform, not the static solid |
| AN-12 | `set_key` on an unset input; again at the same time with another value; at another time; `remove_key` twice; `set_key` on a ref input | one key; replaced; two keys; one key, then a plain literal; `bad_op` |
| AN-13 | `slopc render --frame N` of a project whose solid colour is animated with cut keys per frame | each frame's pixel is that key's colour |
| AN-N1 | `wiggler` frames 24, `range [2, 5]`: same seed twice; frame 0 with `clip 3`; all 24 frames | identical; 3.5; all within 2–5 |
| AN-N2 | `fn_ease` defaults, frames 20, at frames 0, 1, 10, 19; `fn_smoothstep` value 0.25 for each type; `fn_math` multiply 3, 4 | 0, 0.5, 1, 0.5; 0.15625, 0.1, 0.0357143, 0.1464466; 12 |
| AN-N3 | `revert` frames 8, source = cut-keyed colour per frame: frame 0, frame 5 | source frame 7, 2 |
| AN-N4 | `anim_speed` speed 2, frames 8: frame 3 (`loop`), frame 5 (`loop`), frame 5 (`empty`) | source 6; source 2; transparent |
| AN-N5 | `rate_remap` framerate 4 with fps 12, frames 12: frames 0, 2, 3, 7 | source 0, 0, 3, 6 |
| AN-N6 | `delay` frames 2, frames 8: frame 0 (`hold`), 1 (`loop`), 1 (`clear`), 5 | source 0; 7; transparent; 3 |
| AN-N7 | `anim_loop` `loop_start 2`, `loop_range 3`, frames 12: frames 0, 1, 4, 5, 9; `pingpong` frames 4, 5, 6 | source 0 (pre-loop pass), 1, 1, 2, 3; 3, 2, 1 |
| SRV-22 | `GET /api/project?frame=5` on a project with an animated input | `animated` holds the frame-5 value; `keys` differ from frame 0 for the animated node |
| WEB-25 | headless Chromium on a 4-frame animated project: timeline next, play | the frame number advances; the animated node's thumbnail URL carries the frame and its pixels change |
| WEB-26 | headless Chromium: key button on `solid.color` at frame 0, change colour at frame 2 | the file holds an `anim` with keys at 0 and 2; the key button is filled at 2, hollow at 1 |
