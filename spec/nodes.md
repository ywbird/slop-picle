# Nodes

Behavior of each node type. The catalog entry (see [types.md](types.md)) is generated from the tables here.

M1 nodes: `solid`, `noise`, `transform`, `blend`, `image`, `output`. The remaining nodes are specified in M3.

## Following upstream

Node types follow [Pixel Composer](https://github.com/Ttanasart-pt/Pixel-Composer) (`scripts/node_*`, `shaders/sh_*`):

- **Names.** Type = upstream node name in snake case (`Node_Solid` → `solid`). Input and output names = upstream display
  names in snake case (`"Surface In"` → `surface_in`, `"Blend mode"` → `blend_mode`).
- **Defaults and math** are upstream's unless a deviation below says otherwise.
- **Subset.** A node may leave out upstream inputs (masks, UV maps, animation helpers). Left-out inputs behave as their
  upstream default. Each node lists what it leaves out.
- **Deviations** are listed per node with the reason. Global deviations:
  - Random numbers use the integer hash below, not upstream's `fract(sin(...))`, so every GPU gives the same bits
    (user decision, 2026-11-17). Same seed ≠ same pixels as upstream.
  - `output` is not an upstream node (user decision; upstream writes files with `Export`).
  - Sampling is always nearest (upstream's default interpolation for pixel art); no filtering in M1.

`dimension` inputs follow upstream `nodeValue_Dimension`: absent means the project's `width × height`.

## Shared definitions

`hash(u32) → u32` is the PCG hash:

```
state = v * 747796405u + 2891336453u
word  = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u
hash  = (word >> 22u) ^ word
```

All arithmetic wraps modulo 2³². `hash2(x, y, seed) = hash(x ^ hash(y ^ hash(seed)))`, with `x`, `y`, `seed` cast to `u32`
(two's complement for negatives). `unit(h) = (h >> 8) / 2²⁴`, a float in `[0, 1)`, exact in `f32`.

Placement uses fractions of a surface size: `position [0.5, 0.5]` is the centre.

## solid — generate

Upstream: `scripts/node_solid`. Left out: `Foreground`, `Mask`, `Use mask dimension`, `Empty`, `Multiply Alpha`.

| Input | Type | Default | Range |
|---|---|---|---|
| `dimension` | dimension | project size | |
| `color` | color | `#ffffff` | |

| Output | Type |
|---|---|
| `surface_out` | surface, `dimension` |

Every pixel is `color`.

| ID | Given | Expected |
|---|---|---|
| N-SOLID-01 | 4×3 project, `color` `#ff800040` | 4×3 surface, every pixel (255,128,0,64) |
| N-SOLID-02 | no inputs | every pixel (255,255,255,255) |
| N-SOLID-03 | 4×3 project, `dimension [2, 5]` | 2×5 surface |

## noise — generate

Upstream: `scripts/node_noise`, `shaders/sh_noise`. White noise, one random value per pixel.
Left out: `UV Map`, `UV Mix`, `Mask`, `Level In`, `Level Out`. Deviation: integer hash (see above); `seed` is an int
(upstream blends fractional seeds).

| Input | Type | Default | Range |
|---|---|---|---|
| `dimension` | dimension | project size | |
| `seed` | int | 0 | |
| `color_mode` | enum `greyscale`, `rgb`, `hsv` | `greyscale` | |
| `color_r_range` | vec2 | `[0, 1]` | each 0–1 |
| `color_g_range` | vec2 | `[0, 1]` | each 0–1 |
| `color_b_range` | vec2 | `[0, 1]` | each 0–1 |

| Output | Type |
|---|---|
| `surface_out` | surface, `dimension` |

Let `h = hash2(x, y, seed)` and `u_c = unit(hash(h + c))` for channel `c = 0, 1, 2`.

- `greyscale`: pixel = (g, g, g, 255) with `g = h >> 24`. The ranges are ignored. Exact.
- `rgb`: channel `c` = `lo_c + u_c × (hi_c − lo_c)` with `[lo_c, hi_c]` its range; alpha 1.
- `hsv`: the same three values are hue, saturation, value; converted with upstream `hsv2rgb`
  (`shaders/sh_noise`); alpha 1.

| ID | Given | Expected |
|---|---|---|
| N-NOISE-01 | `greyscale`, seed 0, 4×4 | equals golden `noise_grey_s0.png` exactly; every pixel equals the formula computed on the CPU in the test |
| N-NOISE-02 | seeds 0 and 1 | outputs differ |
| N-NOISE-03 | `greyscale`, 64×64 | mean of g in 112–144; all pixels opaque and grey |
| N-NOISE-04 | `rgb`, ranges R `[0,0]`, G `[1,1]`, B `[0,1]` | every pixel R 0, G 255; B equals the CPU formula ± 1 |
| N-NOISE-05 | `hsv`, ranges `[0,1]`, `[0,0]`, `[1,1]` (saturation 0, value 1) | every pixel (255,255,255,255) |
| N-NOISE-06 | `color_r_range [0, 2]` | load error `out_of_range` |
| N-NOISE-07 | `greyscale`, seed 0, `dimension [8, 8]` in a 4×4 project | 8×8; its top-left 4×4 equals N-NOISE-01 |

## transform — transform

Upstream: `scripts/node_transform`, `scripts/point_rotate`. Left out: `Output Dimension Type` (always same as input),
`Round Position`, `Relative Anchor` (always relative), `Rotate by Velocity`, path inputs, stretch, echo,
render mode `Tile`. Extra outputs (`Atlas data`, `Dimension`, velocity) are left out.

| Input | Type | Default | Range |
|---|---|---|---|
| `surface_in` | surface | required | |
| `render_mode` | enum `normal`, `wrap` | `normal` | |
| `position` | vec2 | `[0.5, 0.5]` | fraction of the output size |
| `anchor` | vec2 | `[0.5, 0.5]` | fraction of the input size |
| `rotation` | number | 0 | degrees, **counter-clockwise** on screen (upstream `point_rotate`) |
| `scale` | vec2 | `[1, 1]` | each component ≠ 0 at evaluation, else `eval_error` |
| `alpha` | number | 1 | 0–1 |

| Output | Type |
|---|---|
| `surface_out` | surface, same size as `surface_in` |

The input's anchor point is placed at `position`, then the image is scaled and rotated around it.
With `size` the input size, `P = position × size`, `A = anchor × size`, for output pixel centre `p = (x+0.5, y+0.5)`:
`u = A + S⁻¹ · R⁻¹ · (p − P)`, where `R` rotates counter-clockwise on screen (y down) and `S = diag(scale)`.
The output pixel is input pixel `floor(u)` with its alpha multiplied by `alpha`. Outside the input it is `(0,0,0,0)`;
with `wrap` it is `floor(u) mod size` (always non-negative).

Rotation by a multiple of 90° and integer moves must be exact. Implementations snap `sin`/`cos` of multiples of 90° to
exact 0/±1.

| ID | Given | Expected |
|---|---|---|
| N-TRANSFORM-01 | 4×4 asymmetric fixture, all defaults | output equals input |
| N-TRANSFORM-02 | fixture, `position [0.75, 0.5]` | column 0 transparent; column x equals input column x−1 |
| N-TRANSFORM-03 | fixture, `position [0.75, 0.5]`, `wrap` | column 0 equals input column 3 |
| N-TRANSFORM-04 | fixture, `rotation 90` | output (x, y) equals input (3−y, x) (counter-clockwise) |
| N-TRANSFORM-05 | fixture, `rotation 360` and `-360` | equal input |
| N-TRANSFORM-06 | 4×4 surface with a 2×2 pattern at top-left, `scale [2, 2]`, `anchor [0, 0]`, `position [0, 0]` | each pattern pixel becomes a 2×2 block |
| N-TRANSFORM-07 | fixture, `scale [-1, 1]` | horizontal mirror |
| N-TRANSFORM-08 | `scale [0, 1]` | `eval_error` on the transform node |
| N-TRANSFORM-09 | 5×3 fixture, `rotation 90` | output 5×3, golden `transform_rot90_5x3.png` exactly |
| N-TRANSFORM-10 | opaque fixture, `alpha 0.5` | colours unchanged, alpha 128 ± 1 |

## blend — compose

Upstream: `scripts/node_blend`, `scripts/draw_surface_blend`, `shaders/sh_blend_*`. Left out: `Mask`, `Swap`,
`Invert mask`, `Mask feather`, `Output dimension` (always background), `Constant dimension`, `Fill mode` (always none),
`Horizontal Align`, `Vertical Align`. Blend modes in M1: `normal`, `replace`, `multiply`, `add`, `screen`
(upstream names; the rest come later).

| Input | Type | Default | Range |
|---|---|---|---|
| `background` | surface | required | |
| `foreground` | surface | none (optional) | |
| `blend_mode` | enum `normal`, `replace`, `multiply`, `add`, `screen` | `normal` | |
| `opacity` | number | 1 | 0–1 |
| `preserve_alpha` | bool | false | |
| `position` | vec2 | `[0.5, 0.5]` | fraction of the background size |

| Output | Type |
|---|---|
| `surface_out` | surface, same size as `background` |

Without `foreground`, the output is the background.

Otherwise, as upstream does: a **foreground layer** the size of the background starts fully transparent `(0,0,0,0)`,
and the foreground is copied onto it with its top-left at `floor(position × bg_size − fg_size / 2)` (centred on
`position`). Then every pixel of the output is computed from background `B` and layer `F` (straight RGBA in 0–1), with
`o = opacity`, `al = F.a' + B.a·(1 − F.a')` where `F.a'` is the mode's foreground alpha below:

| Mode | Formula (from `sh_blend_*`) |
|---|---|
| `normal` | `F.a' = F.a·o`; if `al = 0` → `(0,0,0,0)`; else `rgb = (F.rgb·F.a' + B.rgb·B.a·(1 − F.a')) / al`, `a = al` |
| `replace` | `res = mix(B, F, o)` (all four channels) |
| `multiply` | `F.a' = F.a`; `res = mix(B, B·F, o)` (all four channels), then `res.rgb /= al` |
| `add` | `F.a' = F.a·o`; `res = B.rgb·B.a + F.rgb·F.a'` and `res.a = B.a + F.a'`, then `res.rgb /= al` |
| `screen` | `F.a' = F.a`; `F.rgb ·= F.a`; `res = mix(B, 1 − (1 − B)(1 − F), o)` (all four channels), then `res.rgb /= al` |

Then, for every mode: if `preserve_alpha`, `a = B.a`. Results are clamped to 0–1.
Deviation: where upstream divides by `al = 0`, the output is `(0,0,0,0)`.

Note, faithful to upstream: the formulas also run where the layer is transparent, so `multiply` and `screen`
change the background outside the foreground (`multiply` makes it transparent).

| ID | Given | Expected |
|---|---|---|
| N-BLEND-01 | opaque red over opaque blue, same size, `normal` | red |
| N-BLEND-02 | opaque white over opaque black, `opacity 0.5` | (128,128,128,255) ± 1 |
| N-BLEND-03 | fully transparent foreground, `normal` | output equals background |
| N-BLEND-04 | both fully transparent, each mode | (0,0,0,0) |
| N-BLEND-05 | opaque (128,128,128) over opaque (255,0,64), `multiply` | (128,0,32,255) ± 1 |
| N-BLEND-06 | opaque (200,100,0) + opaque (100,200,50), `add` | (255,255,50,255) |
| N-BLEND-07 | opaque (128,0,255) over opaque (128,255,0), `screen` | (192,255,255,255) ± 1 |
| N-BLEND-08 | opaque red over (0,0,0,0), `multiply` | (0,0,0,0) (upstream behavior) |
| N-BLEND-09 | 2×2 foreground on 4×4 background, `normal`, default position | only pixels x, y ∈ {1, 2} change |
| N-BLEND-10 | 8×8 foreground on 4×4 background | output 4×4, showing foreground pixels 2–5 in both axes |
| N-BLEND-11 | no `foreground` | output equals background |
| N-BLEND-12 | opaque red over (0,0,0,0), `normal`, `preserve_alpha` | alpha 0 |
| N-BLEND-13 | 2×2 foreground on 4×4, `position [0, 0]` | only pixel (0, 0) changes; it equals foreground pixel (1, 1) |
| N-BLEND-14 | 2×2 opaque foreground on 4×4 opaque background, `multiply` | pixels outside x, y ∈ {1, 2} become (0,0,0,0) |
| N-BLEND-15 | opaque white over opaque black, `replace`, `opacity 0.25` | (64,64,64,255) ± 1 |

## image — io

Upstream: `scripts/node_image`. Left out: `Padding`; outputs `Path` and `Dimension`.

| Input | Type | Default | Range |
|---|---|---|---|
| `path` | path | required | |

| Output | Type |
|---|---|
| `surface_out` | surface, the file's size |

Reads a PNG file. Any PNG colour type and bit depth is converted to RGBA8 (16-bit channels keep their high byte).
Colour profiles and gamma chunks are ignored. Other formats fail with `eval_error`. Upstream reads more formats;
they come later.

| ID | Given | Expected |
|---|---|---|
| N-IMAGE-01 | RGBA8 PNG fixture | surface bytes equal the PNG's pixel bytes |
| N-IMAGE-02 | palette PNG with a transparent entry, greyscale PNG, 16-bit RGB PNG | converted as described; equal to golden RGBA bytes |
| N-IMAGE-03 | path relative to the project file, project loaded from another working directory | file found |
| N-IMAGE-04 | missing file | `eval_error` naming the path |
| N-IMAGE-05 | a JPEG file renamed to `.png` | `eval_error` |
| N-IMAGE-06 | 5000×1 PNG | `eval_error` (over size limit) |

## output — io

Not an upstream node (see above).

| Input | Type | Default | Range |
|---|---|---|---|
| `surface_in` | surface | required | |

| Output | Type |
|---|---|
| `surface_out` | surface, equal to `surface_in` |

Marks what `slopc render` writes when no `--node` is given (see [cli.md](cli.md)).

| ID | Given | Expected |
|---|---|---|
| N-OUTPUT-01 | `surface_in` = ref to a solid | output equals the solid |
