# Nodes

Behavior of each node type. The catalog entry (see [types.md](types.md)) is generated from the tables here.

M1 nodes: `solid`, `noise`, `transform`, `blend`, `image`, `output`. M3 nodes follow in the second half of this file.

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

Upstream: `scripts/node_blend`, `scripts/draw_surface_blend`, `shaders/sh_blend_*`, `scripts/mask_function`
(`mask_modify`), `shaders/sh_mask_invert`. Left out: `Horizontal Align`, `Vertical Align` (upstream neither shows nor
uses them) and the mask's "alpha only" attribute (off). Blend modes: all of upstream `BLEND_TYPES`
(`scripts/draw_surface_blend`), in its order, names in snake case.

| Input | Type | Default | Range |
|---|---|---|---|
| `background` | surface | required | |
| `foreground` | surface | none (optional) | |
| `mask` | surface | none (optional) | |
| `swap` | bool | false | |
| `invert_mask` | bool | false | |
| `mask_feather` | number | 1 | 1–16 |
| `output_dimension` | enum `background`, `foreground`, `mask`, `maximum`, `constant` | `background` | |
| `constant_dimension` | dimension | project size | |
| `blend_mode` | enum `normal`, `replace`, `multiply`, `color_burn`, `linear_burn`, `minimum`, `add`, `screen`, `color_dodge`, `maximum`, `overlay`, `soft_light`, `hard_light`, `vivid_light`, `linear_light`, `pin_light`, `difference`, `exclusion`, `subtract`, `divide`, `hue`, `saturation`, `luminosity`, `equal`, `unequal` | `normal` | |
| `opacity` | number | 1 | 0–1 |
| `preserve_alpha` | bool | false | |
| `fill_mode` | enum `none`, `stretch`, `tile` | `none` | |
| `position` | vec2 | `[0.5, 0.5]` | fraction of the output size |

| Output | Type |
|---|---|
| `surface_out` | surface, `output_dimension` |

With `swap`, `background` and `foreground` trade places first. The output size is the background's, the foreground's,
the mask's, the largest of the three per axis (`maximum`) or `constant_dimension`; a missing surface counts as 1×1.
Missing background (only possible with `swap`): the output is transparent. The background is stretched to the output
size (nearest-neighbour: output pixel `p` reads `floor((p + 0.5) · size_in / size_out)`), as is the mask. Without
`foreground`, the output is the stretched background.

Otherwise, as upstream does: a **foreground layer** the size of the output starts fully transparent `(0,0,0,0)` and
gets the foreground by `fill_mode`: `none` copies it with its top-left at `floor(position × out_size − fg_size / 2)`
(centred on `position`); `stretch` stretches it to the output size; `tile` repeats it from the top-left corner.

Mask strength `m` (1 without a mask): `(r + g + b) / 3 · a` of the stretched mask pixel; with `invert_mask` the mean
becomes `1 − (r + g + b) / 3` (alpha unchanged, so a transparent mask pixel stays 0, as upstream). `mask_feather` blurs
the strength with the `blur` node's kernel of that size (1: no blur), outside the mask counting as 0 (deviation:
upstream blurs the mask image with its own Gaussian).

Then every pixel of the output is computed from background `B` and layer `F` (straight RGBA in 0–1) with
`o = opacity` and `om = o·m`, as the mode's `sh_blend_*` shader does (`om` where the shader uses `opacity ·
sampleMask()`, `o` where it uses `opacity` alone). Vector operations act on all four channels (alpha too), in the
order written; `al = F.a + B.a·(1 − F.a)` with `F.a` as scaled at that point, `po = preserve_alpha ? F.a : o`,
`lum(c) = 0.2126·c.r + 0.7152·c.g + 0.0722·c.b`:

| Mode | Formula |
|---|---|
| `normal` | `F.a ·= om`; `res = (F·F.a + B·B.a·(1 − F.a)) / al`, `res.a = al` |
| `replace` | `res = mix(B, F, om)` |
| `multiply` | `res = mix(B, B·F, om)`, `res.rgb /= al` |
| `minimum`, `maximum` | `F ·= om`; `res = min(B, F)` / `max(B, F)` per channel |
| `add` | `B.rgb ·= B.a`; `F.a ·= om`; `F.rgb ·= F.a`; `res = B + F`, `res.rgb /= al` |
| `subtract` | `B.rgb ·= B.a`; `F.a ·= om`; `F.rgb ·= F.a`; `res = mix(B, B − F, po)`, `res.rgb /= al` |
| `screen` | `F.rgb ·= F.a`; `res = mix(B, 1 − (1 − B)(1 − F), om)`, `res.rgb /= al` |
| `overlay` | `F.a ·= om`; `X = lum(F.rgb) > 0.5 ? 1 − (1 − 2(F − 0.5))(1 − B) : 2F·B`; `res = mix(B, X, po)`, `res.rgb /= al` |
| `difference` | `res = mix(B, (abs(B.rgb − F.rgb), 1), om)`; `preserve_alpha` has no effect (upstream ignores it) |
| `equal`, `unequal` | `c = om` where `B = F` exactly (`equal`) or where they differ (`unequal`), else `0`; `res = (c, c, c, c)` |
| `hue` | `h = hsv(F.rgb)` with saturation and value of `hsv(B.rgb)`; `res = (mix(B.rgb, rgb(h), F.a·om), B.a)`, `res.rgb /= al` |
| `saturation` | `h = hsv(B.rgb)`, `h.s = mix(h.s, hsv(F.rgb).s, F.a·om)`; `res = (rgb(h), B.a)`, `res.rgb /= al` |
| `luminosity` | `h = hsl(B.rgb)`, `h.l = mix(h.l, hsl(F.rgb).l, F.a·om)`; `res = (rgb(h), B.a)`, `res.rgb /= al` |
| the rest, below | `F.a ·= om`; `mx = B·(1 − o) + X·o`; `res = mix(B, mx, po)`, `res.rgb /= al` |

| Mode | `X` (`L = lum(F.rgb) > 0.5`) |
|---|---|
| `color_burn` | `1 − (1 − B) / F` |
| `linear_burn` | `B + F − 1` |
| `color_dodge` | `F / (1 − B)` |
| `soft_light` | `L ? 1 − (1 − B)(1 − (F − 0.5)) : B·(F + 0.5)` |
| `hard_light` | `L ? 1 − (1 − B)(1 − 2(F − 0.5)) : 2B·F` |
| `vivid_light` | `L ? 1 − (1 − B)·2(F − 0.5) : B / (1 − 2F)` |
| `linear_light` | `B + 2F − 1` (upstream's two branches are equal) |
| `pin_light` | `L ? max(B, 2(F − 0.5)) : min(B, 2F)` |
| `exclusion` | `0.5 − 2(B − 0.5)(F − 0.5)` |
| `divide` | `B / F` |

`hsv` and `rgb` are upstream's `rgb2hsv` / `hsv2rgb` (as `color_adjust`); `hsl` is upstream's `rgb2hsl` / `hsl2rgb`.
Then, for every mode but `difference`: if `preserve_alpha`, `a = B.a`. Results are clamped to 0–1.
Deviations: where upstream divides by `al = 0`, the output is `(0,0,0,0)`; any other `x / 0` is `0` for `x = 0`, else
`±10³⁰` (the GPU's result for a division by 0 varies).

Faithful to upstream, the formulas act on alpha as well, so some modes give a transparent result for two opaque
inputs (for example `exclusion`, `subtract`, and `vivid_light` with a dark foreground): `preserve_alpha` keeps the
background's alpha. Likewise the mask only scales `F.a` in `overlay`, `subtract` and the modes of the second table, so
over an opaque background at full opacity it changes nothing there.

Note, faithful to upstream: the formulas also run where the layer is transparent, so `multiply` and `screen`
change the background outside the foreground (`multiply` makes it transparent).

| ID | Given | Expected |
|---|---|---|
| N-BLEND-01 | opaque red over opaque blue, same size, `normal` | red |
| N-BLEND-02 | opaque white over opaque black, `opacity 0.5` | (128,128,128,255) ± 1 |
| N-BLEND-03 | fully transparent foreground, `normal` | output equals background |
| N-BLEND-04 | both fully transparent, each mode | (0,0,0,0); `difference` (0,0,0,255), `equal` (255,255,255,255) |
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
| N-BLEND-16 | opaque `B` (200,100,50) under opaque dark `F` (60,90,40) or light `F` (230,200,120), each mode below | the table below, ± 2 |
| N-BLEND-17 | opaque (200,100,50) over opaque black, `divide` | (255,255,255,255) (division by 0) |
| N-BLEND-18 | the same colour as background and foreground, `equal`; `unequal`; `equal` with `opacity 0.5` | (255,255,255,255); (0,0,0,0); (128,128,128,128) ± 1 |
| N-BLEND-19 | 2×2 black background, white foreground, mask white in column 0 and black in column 1 | column 0 white, column 1 black |
| N-BLEND-20 | N-BLEND-19 with `invert_mask` | column 0 black, column 1 white |
| N-BLEND-21 | N-BLEND-19 with a fully transparent mask, `invert_mask` | black (strength 0) |
| N-BLEND-22 | 6×1, mask white in x < 3, black after, `mask_feather 4` | x 0 white; x 2 and 3 strictly between black and white; x 5 below 64 |
| N-BLEND-23 | opaque red 4×4 background, opaque blue 2×2 foreground, `swap` | 2×2 output, all red |
| N-BLEND-24 | 2×2 background (four colours), 4×4 foreground transparent, `output_dimension foreground` | 4×4; each background pixel covers a 2×2 block |
| N-BLEND-25 | background 4×2, foreground 2×4, `output_dimension maximum`; `constant` with `constant_dimension [3, 5]`; `mask` without a mask | 4×4; 3×5; 1×1 |
| N-BLEND-26 | 2×2 foreground (four colours) on a 4×4 background, `fill_mode stretch` | each foreground pixel covers a 2×2 block |
| N-BLEND-27 | the same, `fill_mode tile` | the 2×2 pattern at (0,0), (2,0), (0,2), (2,2) |
| N-BLEND-28 | `swap` with no foreground | 1×1, (0,0,0,0) |
| N-BLEND-29 | `color_burn`, light `F` of N-BLEND-16 over its `B`, a 50% grey mask | (194,57,0,255), as without a mask (upstream) |

N-BLEND-16 expectations (from upstream's formulas; `o` = opacity):

| Mode | `F` | `o` | Result |
|---|---|---|---|
| `color_burn` | light | 1 | (194,57,0,255) |
| `linear_burn` | light | 1 | (175,45,0,255) |
| `minimum` | light | 0.5 | (115,100,50,128) |
| `color_dodge` | dark | 1 | (255,148,50,255) |
| `maximum` | light | 0.5 | (200,100,60,255) |
| `overlay` | dark; light | 1 | (94,71,16,255); (244,188,38,255) |
| `soft_light` | dark; light | 1 | (147,85,33,255); (222,144,44,255) |
| `hard_light` | light | 1 | (244,188,38,255) |
| `vivid_light` | dark; light | 1 | (255,255,73,0); (211,167,255,255) |
| `linear_light` | dark | 1 | (65,25,0,255) |
| `pin_light` | dark; light | 1 | (120,100,50,255); (205,145,50,255) |
| `difference` | light | 0.5 | (115,100,60,255) |
| `exclusion` | light | 1 | (69,143,123,0) |
| `subtract` | dark | 1 | (140,10,10,0) |
| `divide` | light | 1 | (222,128,106,255) |
| `hue` | dark | 1 | (110,200,50,255) |
| `saturation` | light | 1 | (200,136,104,255) |
| `luminosity` | dark | 1 | (104,52,26,255) |

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

---

# M3 nodes

Same rules as above. Shared notation: `uv = ((x + 0.5)/w, (y + 0.5)/h)`; `p = (x + 0.5, y + 0.5)` in pixels;
`lum(c) = 0.2126 r + 0.7152 g + 0.0722 b`; angles in degrees; `R(a)` rotates counter-clockwise on screen
(`x' = x cos a − y sin a`, `y' = x sin a + y cos a` with y down, as upstream shaders write it). Multiples of 90° snap
to exact `sin`/`cos` (as in `transform`). Unless stated, outputs are named `surface_out` and have the input's size.
Tests use a tolerance of 1 per channel wherever float math is involved.

Global deviations for M3, besides those above:

- Masks, UV maps, `Mix`, channel toggles and per-input map surfaces (`setMappable`) are left out everywhere.
- Upstream `DEF_PALETTE` was not located; palette defaults are `["#000000", "#ffffff"]`.
- Colour distances are RGB Euclidean everywhere (upstream `dither` uses LAB).

## gradient — generate

Upstream: `Node_Gradient` ("Draw Gradient"), `shaders/sh_gradient`. Left out: progress/inverse/level curves,
`Shape`, `Diamond` type, levels.

| Input | Type | Default | Range |
|---|---|---|---|
| `dimension` | dimension | project size | |
| `gradient` | gradient | black → white, `rgb` | |
| `shift` | number | 0 | −2–2 |
| `scale` | number | 1 | 0–5 |
| `loop` | enum `none`, `loop`, `pingpong` | `none` | |
| `type` | enum `linear`, `circular`, `radial` | `linear` | |
| `angle` | number | 0 | |
| `radius` | number | 0.5 | |
| `center` | vec2 | `[0.5, 0.5]` | fraction of size |
| `uniform_ratio` | bool | true | |

With `d = uv − center`, `a = angle`:
`linear`: `t = 0.5 + d.x cos a − d.y sin a`; `circular`: `t = |d ⊙ A| / (radius·√2)` with `A = (w/h, 1)` if
`uniform_ratio` else `(1, 1)`; `radial`: `t = ((atan2(d.y, d.x) + a) mod 2π) / 2π`.
Then `t = (t + shift − 0.5)/scale + 0.5`; `loop`: `fract(t)`; `pingpong`: `1 − |mod(t, 2) − 1|`. Pixel = `gradient(t)`.

| ID | Given | Expected |
|---|---|---|
| N-GRADIENT-01 | 8×1, defaults | pixel x grey `round((x + 0.5)/8 × 255)` ± 1, opaque |
| N-GRADIENT-02 | 4×1, `constant` keys 0 black, 0.5 white | x = 0, 1 black; x = 2, 3 white |
| N-GRADIENT-03 | 4×4, `circular` | centre pixels darker than corners |
| N-GRADIENT-04 | 8×1, `shift 0.5`, `loop` | pixel 7 grey `round(fract(0.9375 + 0.5) × 255)` ± 1 |

## shape — generate

Upstream: `Node_Shape` ("Draw Shape"), `shaders/sh_shape`. Only positioning mode `Center + Scale` and shapes
`rectangle`, `ellipse`; no anti-aliasing (upstream default). Background `surface` is left out (with none connected it
equals `none`). Outputs `Mask`, `Height`, `UV` are left out.

| Input | Type | Default | Range |
|---|---|---|---|
| `dimension` | dimension | project size | |
| `shape` | enum `rectangle`, `ellipse` | `rectangle` | |
| `center` | vec2 | `[0.5, 0.5]` | fraction of size |
| `half_size` | vec2 | `[0.5, 0.5]` | fraction of size |
| `shape_rotation` | number | 0 | |
| `shape_color` | color | `#ffffff` | |
| `background` | enum `none`, `solid` | `none` | |
| `bg_color` | color | `#000000` | |

| Output | Type |
|---|---|
| `colored` | surface, `dimension` |

`q = R(−shape_rotation)(p − center ⊙ size)`, `H = half_size ⊙ size`. Inside: rectangle `|q.x| ≤ H.x and |q.y| ≤ H.y`;
ellipse `(q.x/H.x)² + (q.y/H.y)² ≤ 1`. Inside → `shape_color`; outside → `bg_color` if `solid`, else `(0,0,0,0)`.

| ID | Given | Expected |
|---|---|---|
| N-SHAPE-01 | 8×8, `half_size [0.25, 0.25]` | pixels with x, y ∈ 2..5 white; others transparent |
| N-SHAPE-02 | 8×8, `ellipse` | corners transparent; centre 4 pixels white; symmetric in x and y |
| N-SHAPE-03 | `background solid`, `bg_color #ff0000`, small rectangle | outside pixels red |
| N-SHAPE-04 | 8×8, `half_size [0.5, 0.25]`, `shape_rotation 90` | filled area is 4 wide, 8 tall |

## cellular — generate

Upstream: `Node_Cellular` ("Cellular Noise"), `shaders/sh_cell_noise`. Only type `Point`, patterns `tiled`, `uniform`.
Left out: `Radial` pattern, `Gap`, `Size`, iterations, levels, `Colored`. Deviation: integer hash for feature points.

| Input | Type | Default | Range |
|---|---|---|---|
| `dimension` | dimension | project size | |
| `seed` | int | 0 | |
| `pattern` | enum `tiled`, `uniform` | `tiled` | |
| `phase` | number | 0 | degrees |
| `randomness` | number | 1 | 0–1 |
| `position` | vec2 | `[0.5, 0.5]` | fraction of size |
| `rotation` | number | 0 | |
| `scale` | number | 4 | ≥ 0 |
| `contrast` | number | 1 | 0–4 |
| `middle` | number | 0.5 | 0–1 |
| `inverted` | bool | false | |

`n = (uv.x, uv.y · h/w)`, `st = R(rotation)(n − position) · s` with `s = floor(scale)` for `tiled`, `scale` otherwise.
`i = floor(st)`, `f = fract(st)`. Over the 3×3 neighbours `k`: cell `c = i + k` (for `tiled`, `c mod s`),
`h = hash2(c.x, c.y, seed)`, `r = (unit(h), unit(hash(h)))`, `pt = 0.5 + 0.5 sin(2π fract(r + phase/360)) · randomness`,
`md = min |k + pt − f|`. `v = middle + (md − middle) · contrast`, inverted → `1 − v`. Pixel = `(v, v, v, 1)`.

| ID | Given | Expected |
|---|---|---|
| N-CELLULAR-01 | same seed twice / seeds 0 and 1 | identical / different |
| N-CELLULAR-02 | 16×16, `randomness 0`, `position [0, 0]`, `scale 4` | periodic: pixel (x, y) equals (x + 4, y) and (x, y + 4) |
| N-CELLULAR-03 | `inverted` | each grey value is 255 − the non-inverted value |
| N-CELLULAR-04 | `contrast 0` | every pixel grey 128 |

## checker — generate

Upstream: `Node_Checker`, `shaders/sh_checkerboard`. Type `Solid` only. Left out: `Diagonal`, `Aspect`.

| Input | Type | Default | Range |
|---|---|---|---|
| `dimension` | dimension | project size | |
| `size` | number | 0.5 | > 0; cell size as a fraction of the width |
| `angle` | number | 0 | |
| `position` | vec2 | `[0.5, 0.5]` | fraction of size |
| `color_1` | color | `#ffffff` | |
| `color_2` | color | `#000000` | |

`q = R(angle)(p − position ⊙ size_px)`, `S = size · w`. `k = floor(q.x/S) + floor(q.y/S)`; even → `color_2`,
odd → `color_1`.

| ID | Given | Expected |
|---|---|---|
| N-CHECKER-01 | 8×8, defaults | four 4×4 cells; top-left black, top-right white |
| N-CHECKER-02 | 8×8, `size 0.25` | 2×2 cells alternating |
| N-CHECKER-03 | `color_1 #ff0000`, `color_2 #0000ff` | only those colours |

## stripe — generate

Upstream: `Node_Stripe`, `shaders/sh_stripe`. Type `Solid`, coloring `Alternate` only. Left out: seed, `Tiled`,
`Amount`, `Random`, gradient and palette coloring.

| Input | Type | Default | Range |
|---|---|---|---|
| `dimension` | dimension | project size | |
| `size` | number | 0.25 | > 0; fraction of the height |
| `strip_ratio` | number | 0.5 | 0–1 |
| `angle` | number | 0 | |
| `position` | vec2 | `[0.5, 0.5]` | fraction of size |
| `progress` | number | 0.5 | 0–1 |
| `color_1` | color | `#ffffff` | |
| `color_2` | color | `#000000` | |

`d = p − position ⊙ size_px`, `t = (d.x cos a − d.y sin a) / (size · h)`, `s = fract(t + progress)`;
`s ≥ strip_ratio + 0.001` → `color_1`, else `color_2`.

| ID | Given | Expected |
|---|---|---|
| N-STRIPE-01 | 8×8, defaults | columns alternate per pixel, column 0 white |
| N-STRIPE-02 | 8×8, `angle 90` | rows alternate per pixel, row 0 black |
| N-STRIPE-03 | `strip_ratio 1` | all `color_2` |

## scale — transform

Upstream: `Node_Scale`. Left out: `Fit Mode`, `Move Atlas`.

| Input | Type | Default | Range |
|---|---|---|---|
| `surface_in` | surface | required | |
| `mode` | enum `upscale`, `scale_to_size` | `upscale` | |
| `scale` | number | 1 | > 0 at evaluation |
| `target_dimension` | dimension | project size | |

Output size: `upscale` → `(round(w·scale), round(h·scale))`; `scale_to_size` → `target_dimension`. Out-of-limit sizes
and `scale ≤ 0` fail with `eval_error`. Pixel `(x, y)` = input `(floor((x + 0.5)·w/W), floor((y + 0.5)·h/H))`.

| ID | Given | Expected |
|---|---|---|
| N-SCALE-01 | 2×2 fixture, `scale 2` | 4×4; each pixel a 2×2 block |
| N-SCALE-02 | 4×4 fixture, `scale 0.5` | 2×2 = input pixels (1,1), (3,1), (1,3), (3,3) |
| N-SCALE-03 | 6×2 fixture, `scale_to_size [3, 1]` | 3×1 = input (1,1), (3,1), (5,1) |
| N-SCALE-04 | `scale 0` | `eval_error` |

## crop — transform

Upstream: `Node_Crop`, aspect ratio `Any` only.

| Input | Type | Default | Range |
|---|---|---|---|
| `surface_in` | surface | required | |
| `crop` | padding | `[0, 0, 0, 0]` | right, top, left, bottom |

Output `(w − right − left, h − top − bottom)`, each ≥ 1 else `eval_error`; pixel = input `(x + left, y + top)`.

| ID | Given | Expected |
|---|---|---|
| N-CROP-01 | 4×2 fixture, `[1, 0, 1, 0]` | 2×2 = input columns 1, 2 |
| N-CROP-02 | 4×4 fixture, `[0, 1, 0, 2]` | 4×1 = input row 1 |
| N-CROP-03 | 2×2, `[1, 0, 1, 0]` | `eval_error` |

## flip — transform

Upstream: `Node_Flip`, `shaders/sh_flip`. `axis` is upstream's toggle as an int bit mask.

| Input | Type | Default | Range |
|---|---|---|---|
| `surface_in` | surface | required | |
| `axis` | int | 1 | 0–3; bit 1 = x, bit 2 = y |

Pixel = input `(axis & 1 ? w−1−x : x, axis & 2 ? h−1−y : y)`.

| ID | Given | Expected |
|---|---|---|
| N-FLIP-01 | fixture, default | horizontal mirror |
| N-FLIP-02 | `axis 2` | vertical mirror |
| N-FLIP-03 | `axis 3` | both |
| N-FLIP-04 | `axis 0` | unchanged |

## offset — transform

Upstream: `Node_Offset`, `shaders/sh_offset`. Left out: `Angle`.

| Input | Type | Default | Range |
|---|---|---|---|
| `surface_in` | surface | required | |
| `x_offset` | number | 0 | −1–1 |
| `y_offset` | number | 0 | −1–1 |

Pixel = input `floor(fract(uv + offset) ⊙ size)` (wraps).

| ID | Given | Expected |
|---|---|---|
| N-OFFSET-01 | 4×1 fixture, `x_offset 0.25` | output x = input (x + 1) mod 4 |
| N-OFFSET-02 | 4×4 fixture, `y_offset -0.5` | output y = input (y + 2) mod 4 |

## downscale — transform

Upstream: `Node_Downscale`, `shaders/sh_downscale`. Left out: `Multiply Alpha`.

| Input | Type | Default | Range |
|---|---|---|---|
| `surface_in` | surface | required | |
| `mode` | enum `mix`, `max`, `min` | `mix` | |
| `downscale` | number | 1 | ≥ 1 |

Output `(max(1, ceil(w/s)), max(1, ceil(h/s)))`. For output pixel `(X, Y)`, the samples are input pixels
`floor((X·s + i + 0.5), (Y·s + j + 0.5))` for integers `0 ≤ i, j < s` that fall inside the input.
`mix`: `c = Σ c·c.a` (all four channels), `rgb = c.rgb / Σ a`, `a = c.a / n` (upstream quirk: alpha is the mean of a²);
no opaque sample → `(0,0,0,0)`. `max` / `min`: per-channel max / min; `min` keeps the alpha of input pixel
`floor(uv ⊙ size_in)`.

| ID | Given | Expected |
|---|---|---|
| N-DOWNSCALE-01 | 4×4 opaque, 2×2 blocks of one colour each, `downscale 2` | 2×2 of the block colours |
| N-DOWNSCALE-02 | 4×4 all `(200,100,0,128)`, `downscale 2` | colour (200,100,0), alpha `round(0.502² × 255)` = 64 ± 1 |
| N-DOWNSCALE-03 | 2×2 with values 10, 20, 30, 40 (grey, opaque), `max` / `min` | 1×1: 40 / 10 |
| N-DOWNSCALE-04 | 5×5, `downscale 2` | 3×3 |

## blur — filter

Upstream: `Node_Blur`, `shaders/sh_blur_gaussian`. Gaussian only. Deviation: the kernel is defined here
(upstream `__gaussian_get_kernel` not ported): radius `r = max(ceil(size) − 1, 0)`, `σ = max(size, 1)/3`,
`w(i) = exp(−i²/(2σ²))`. Left out: `Gamma Correction`, `Aspect Ratio`, `Direction`, custom intensity.

| Input | Type | Default | Range |
|---|---|---|---|
| `surface_in` | surface | required | |
| `size` | number | 8 | ≥ 0 |
| `override_color` | bool | false | |
| `color` | color | `#000000` | |

Two passes, horizontal then vertical, each over offsets `−r..r` (outside the image counts as transparent):
`rgb = Σ w a c / Σ w a`, `a = Σ w a / Σ w` (0 alpha sum → `(0,0,0,0)`). `override_color` replaces rgb with `color.rgb`
and multiplies alpha by `color.a`.

| ID | Given | Expected |
|---|---|---|
| N-BLUR-01 | `size 0` | unchanged |
| N-BLUR-02 | 8×8 opaque single colour, `size 3` | centre pixels unchanged; edges keep colour, lower alpha |
| N-BLUR-03 | one opaque white pixel in 9×9 transparent, `size 3` | centre alpha highest; symmetric; rgb white where alpha > 0 |
| N-BLUR-04 | `override_color`, `color #ff0000` | every pixel with alpha > 0 is red |

## outline — filter

Upstream: `Node_Outline`, `shaders/sh_outline`. Positions `inside`, `outside` (no `middle`). Left out: `Texture`,
`Crop border`, anti-aliasing, high res, blend, oversample modes, second output `Outline`.

| Input | Type | Default | Range |
|---|---|---|---|
| `surface_in` | surface | required | |
| `position` | enum `inside`, `outside` | `outside` | |
| `profile` | enum `circle`, `square`, `diamond` | `circle` | |
| `width` | number | 0 | ≥ 0, floored |
| `start` | int | 0 | ≥ 0 |
| `threshold` | number | 1 | 0–1 |
| `color` | color | `#ffffff` | |

`solid(q) = a(q) > 1 − threshold`, `empty(q) = a(q) < threshold` (outside the image is empty). Distance: circle
Euclidean, square Chebyshev, diamond Manhattan. `outside`: an empty pixel with a solid pixel at distance `d`,
`start < d ≤ start + width`, becomes `color`. `inside`: a solid pixel with an empty pixel in that range becomes
`color`. Other pixels are unchanged.

| ID | Given | Expected |
|---|---|---|
| N-OUTLINE-01 | 5×5 transparent with opaque centre, `width 1`, `square` | the 8 neighbours white; rest unchanged |
| N-OUTLINE-02 | same, `circle` and `diamond` | only the 4 edge neighbours white |
| N-OUTLINE-03 | `width 0` | unchanged |
| N-OUTLINE-04 | 5×5 with opaque 3×3 block at centre, `inside`, `width 1`, `square` | the block's 8 border pixels white; centre unchanged |

## shadow — filter

Upstream: `Node_Shadow`, `shaders/sh_shadow_apply`. Side `outside`, positioning `shift` only. Deviation: the shadow
mask is built here (upstream uses several passes not ported): `m = blur(dilate(alpha shifted by shift, grow), blur)`
with `dilate` a Chebyshev max over radius `grow` and `blur` the `blur` node's kernel with `size = blur`. Computed on
the CPU. Left out: `Light Position`, `Remove Original`, `Inside`, output `Shadow Only`.

| Input | Type | Default | Range |
|---|---|---|---|
| `surface_in` | surface | required | |
| `color` | color | `#000000` | |
| `strength` | number | 0.5 | 0–2 |
| `grow` | int | 3 | 0–16 |
| `blur` | int | 3 | 0–16 |
| `shift` | vec2 | `[0, 0]` | pixels |

`shad = (color.rgb, color.a · m) · strength` (all four channels); `out = shad · (1 − base.a) + base · base.a`, clamped.

| ID | Given | Expected |
|---|---|---|
| N-SHADOW-01 | fully opaque input | unchanged |
| N-SHADOW-02 | one opaque white pixel at (1, 1) in 6×3, `grow 0`, `blur 0`, `shift [2, 0]`, `strength 1` | (3, 1) black opaque; (1, 1) white; others transparent |
| N-SHADOW-03 | defaults, a small opaque square | pixels near the square get alpha > 0 and black rgb |

## posterize — filter

Upstream: `Node_Posterize`, `shaders/sh_posterize`, `sh_posterize_palette`. Deviation: `Use Global Range` is off (range
0–1). Left out: `Space` (RGB only), `Reference`, `Hue Bias`.

| Input | Type | Default | Range |
|---|---|---|---|
| `surface_in` | surface | required | |
| `use_palette` | bool | true | |
| `palette` | palette | `["#000000", "#ffffff"]` | |
| `steps` | int | 4 | 2–16 |
| `gamma` | number | 1 | 0–2 |
| `posterize_alpha` | bool | true | |

Palette: `c' = posterize_alpha ? c · c.a : c` (all four channels); result = the palette colour with the smallest
`|pal.rgb − c'.rgb|` (first on ties); if not `posterize_alpha`, its alpha becomes `c.a`.
Steps: `g = max(gamma, 0.0001)`, `rgb = clamp(pow(floor(pow(rgb, g) · steps)/(steps − 1), 1/g), 0, 1)`;
alpha `= posterize_alpha ? 1 : c.a`.

| ID | Given | Expected |
|---|---|---|
| N-POSTERIZE-01 | greys 100, 200 (opaque) and a transparent pixel, default palette | black, white, black opaque |
| N-POSTERIZE-02 | `use_palette false`, greys 128 and 255 | 170 and 255 |
| N-POSTERIZE-03 | `posterize_alpha false`, alpha 100 | alpha 100 kept |

## dither — filter

Upstream: `Node_Dither`, `shaders/sh_dither`. Map mode `Positional`, mode `Color`, colour types `greyscale` and
`palette`. Patterns renamed to snake case: `bayer_2x2`, `bayer_4x4`, `bayer_8x8`, `white_noise`. Left out: `Custom`,
`Matrix`, `Scale`, RGB/HSV types. Deviation: white noise uses the integer hash.

| Input | Type | Default | Range |
|---|---|---|---|
| `surface_in` | surface | required | |
| `seed` | int | 0 | |
| `pattern` | enum `bayer_2x2`, `bayer_4x4`, `bayer_8x8`, `white_noise` | `bayer_2x2` | |
| `invert` | bool | false | |
| `contrast` | number | 1 | 1–5 |
| `color_type` | enum `greyscale`, `palette` | `palette` | |
| `steps` | int | 4 | 2–16 |
| `palette` | palette | `["#000000", "#ffffff"]` | |

Greyscale: `c1 = floor(c·steps)/steps`, `c2 = ceil(c·steps)/steps` per channel, alpha `c.a`; exact if
`|c − c1| < 0.05` (4-channel distance). Palette: `c1`, `c2` = the closest and second-closest colours; exact if a colour is
within 0.001 (rgb replaced by it). Exact → output `c`. Otherwise `d1 = |c − c1|`, `d2 = |c − c2|` (rgb),
`r = (d1/(d1 + d2) − 0.5) · contrast + 0.5`; threshold `v` = Bayer `M[y mod n][x mod n] / (n² − 1)` (standard
matrices; 2×2 = `[[0, 2], [3, 1]]`) or `unit(hash2(x, y, seed))`; `invert` → `1 − v`. `r < v` → `c1`, else `c2`.

| ID | Given | Expected |
|---|---|---|
| N-DITHER-01 | 4×4 grey 128, default palette | each 2×2 block has 2 white and 2 black; (0, 0) black |
| N-DITHER-02 | pixels exactly black or white | unchanged |
| N-DITHER-03 | `invert` | black and white swap places versus N-DITHER-01 |
| N-DITHER-04 | `greyscale`, `steps 4`, grey 64 | each pixel is 63 or 64 ± 1 (`floor`/`ceil` levels or exact) |

## threshold — filter

Upstream: `Node_Threshold`, `shaders/sh_threshold`. Algorithm `Simple`; `Apply to Alpha` `None`. Duplicate upstream
names use their internal names (`brightness_threshold`, `alpha_threshold`, …). Left out: `Multiply`, adaptive.

| Input | Type | Default | Range |
|---|---|---|---|
| `surface_in` | surface | required | |
| `brightness` | bool | false | |
| `brightness_threshold` | number | 0.5 | 0–1 |
| `brightness_smoothness` | number | 0 | 0–1 |
| `brightness_invert` | bool | false | |
| `alpha` | bool | false | |
| `alpha_threshold` | number | 0.5 | 0–1 |
| `alpha_smoothness` | number | 0 | 0–1 |
| `alpha_invert` | bool | false | |

`f(v, t, s) = s = 0 ? (v ≥ t ? 1 : 0) : smoothstep(t − s, t + s, v)`. Brightness: `rgb = f(lum)` (inverted if set).
Alpha: `a = f(a)` (inverted if set).

| ID | Given | Expected |
|---|---|---|
| N-THRESHOLD-01 | defaults | unchanged |
| N-THRESHOLD-02 | `brightness`, greys 100 and 200 | 0 and 255; alpha kept |
| N-THRESHOLD-03 | `alpha`, alphas 100 and 200 | 0 and 255 |
| N-THRESHOLD-04 | `brightness`, `brightness_invert` | 255 and 0 |

## displace — filter

Upstream: `Node_Displace`. Mode `Linear` only, oversample `Empty`. Left out: second map, `Separate Axis`, iterate,
angle offset, blend modes.

| Input | Type | Default | Range |
|---|---|---|---|
| `surface_in` | surface | required | |
| `displace_map` | surface | none (optional) | |
| `strength` | number | 1 | |
| `mid_value` | number | 0.5 | 0–1 |
| `position` | vec2 | `[1, 0]` | fraction of size: the displacement vector |

Without a map, output = input. Otherwise `m = lum(map(floor(uv ⊙ map_size)))`,
`off = position ⊙ size · (m − mid_value) · strength`, pixel = input `floor(p + off)` (transparent outside).

| ID | Given | Expected |
|---|---|---|
| N-DISPLACE-01 | 4×1 fixture, white map, `position [0.5, 0]` | output x = input x + 1; x = 3 transparent |
| N-DISPLACE-02 | map grey 128, `mid_value` = 128/255 (the map's lum) | unchanged |
| N-DISPLACE-03 | no map | unchanged |

## color_adjust — color

Upstream: `Node_Color_adjust` ("Color Adjust"), `shaders/sh_color_adjust`. Blend mode `normal` only. Left out:
`Input Type` colour mode, mask, output `Color Out`.

| Input | Type | Default | Range |
|---|---|---|---|
| `surface_in` | surface | required | |
| `brightness` | number | 0 | −1–1 |
| `contrast` | number | 0.5 | 0–1 |
| `exposure` | number | 1 | 0–4 |
| `hue` | number | 0 | −1–1 |
| `saturation` | number | 0 | −1–1 |
| `value` | number | 0 | −1–1 |
| `blend` | color | `#ffffff` | |
| `blend_amount` | number | 0 | 0–1 |
| `alpha` | number | 1 | 0–1 |

As upstream: `c = clamp(0.5 + 2·contrast·(c − 0.5))` (all channels), `rgb = clamp(rgb + brightness)`,
`c = clamp(c · exposure)`; HSV: `h = fract(h + hue)`, `v = clamp((v + value)(1 + saturation·s·0.5))`,
`s = clamp(s(1 + saturation))`; back to RGB with the original alpha; `rgb = mix(rgb, blend.rgb, blend_amount)`;
`a = a · alpha`.

| ID | Given | Expected |
|---|---|---|
| N-COLOR_ADJUST-01 | defaults on the fixture | unchanged ± 1 |
| N-COLOR_ADJUST-02 | `brightness 0.2` on grey 100 | 151 ± 1 |
| N-COLOR_ADJUST-03 | `hue 0.5` on red | cyan ± 1 |
| N-COLOR_ADJUST-04 | `alpha 0.5` on opaque | alpha 128 ± 1 |
| N-COLOR_ADJUST-05 | `blend #00ff00`, `blend_amount 1` | green, alpha kept |

## invert — color

Upstream: `Node_Invert`, `shaders/sh_invert`.

| Input | Type | Default | Range |
|---|---|---|---|
| `surface_in` | surface | required | |
| `include_alpha` | bool | false | |

`rgb = 1 − rgb`; with `include_alpha`, also `a = 1 − a`.

| ID | Given | Expected |
|---|---|---|
| N-INVERT-01 | (10, 20, 30, 200) | (245, 235, 225, 200) |
| N-INVERT-02 | `include_alpha` | (245, 235, 225, 55) |

## color_replace — color

Upstream: `Node_Color_replace` ("Replace Palette"), `shaders/sh_palette_replace`. Mode `Order` only. Left out:
randomize inputs, `Replace Other Colors`, `Target Color`.

| Input | Type | Default | Range |
|---|---|---|---|
| `surface_in` | surface | required | |
| `from` | palette | `["#000000", "#ffffff"]` | |
| `to` | palette | `["#000000", "#ffffff"]` | |
| `threshold` | number | 0.1 | 0–1 |
| `multiply_alpha` | bool | true | |
| `hard_replace` | bool | true | |

`hsv = rgb2hsv(c.rgb)` (times `c.a` if `multiply_alpha`). The `from` colour with the smallest `|hsv − hsv(from_i)|`
below `threshold` is matched (first on ties); `to` index `round(i/(n_from − 1)·(n_to − 1))` (0 if `n_from = 1`).
Matched: `hard_replace` → that colour; else `mix(to, c, d/threshold)`. Unmatched → `c`. Alpha stays `c.a`.

| ID | Given | Expected |
|---|---|---|
| N-COLOR_REPLACE-01 | `from ["#ff0000"]`, `to ["#0000ff"]`, `multiply_alpha false`, red pixel alpha 200 and a green pixel | blue alpha 200; green unchanged |
| N-COLOR_REPLACE-02 | defaults (same palettes) | black stays black, white stays white |
| N-COLOR_REPLACE-03 | `from` 2 colours, `to` 3 colours | `from[1]` maps to `to[2]` |

## colorize — color

Upstream: `Node_Colorize`, `shaders/sh_colorize`.

| Input | Type | Default | Range |
|---|---|---|---|
| `surface_in` | surface | required | |
| `gradient` | gradient | black → white | |
| `gradient_shift` | number | 0 | −1–1 |
| `color_range` | vec2 | `[0, 1]` | |
| `overflow` | enum `clamp`, `loop` | `clamp` | |
| `multiply_alpha` | bool | true | |
| `keep_alpha` | bool | true | |

`t = (lum(c) + gradient_shift − range.x)/(range.y − range.x)`; clamp or `fract`; `multiply_alpha` → `t ·= c.a`;
`t > 1` → `t = t == floor(t) ? 1 : fract(t)`. Pixel = `gradient(t)`; `keep_alpha` → alpha `c.a`.

| ID | Given | Expected |
|---|---|---|
| N-COLORIZE-01 | gradient black → red, opaque grey 128 | (128, 0, 0, 255) ± 1 |
| N-COLORIZE-02 | transparent white pixel | first key colour, alpha 0 |
| N-COLORIZE-03 | `keep_alpha false`, gradient key alpha 0x80 | alpha from the gradient |

## greyscale — color

Upstream: `Node_Greyscale`, `shaders/sh_greyscale`.

| Input | Type | Default | Range |
|---|---|---|---|
| `surface_in` | surface | required | |
| `brightness` | number | 0 | −1–1 |
| `contrast` | number | 1 | −1–4 |

`rgb = (rgb + brightness)·contrast`; `rgb = lum(rgb) · a` (upstream multiplies by alpha); alpha kept.

| ID | Given | Expected |
|---|---|---|
| N-GREYSCALE-01 | opaque red | grey 54 ± 1 |
| N-GREYSCALE-02 | white, alpha 128 | grey 128 ± 1, alpha 128 |

## number — value

Upstream: `Node_Number`. Only `Value`; display inputs left out.

| Input | Type | Default | Range |
|---|---|---|---|
| `value` | number | 0 | |

| Output | Type |
|---|---|
| `number` | number |

## color — value

Upstream: `Node_Color`. Input `color` (color, default `#ffffff`); output `color` (color) equal to it.

## palette — value

Upstream: `Node_Palette`. Input `palette` (palette, default `["#000000", "#ffffff"]`); output `palette` (palette).
Left out: `Trim range`.

## math — value

Upstream: `Node_Math`. Operations needing more inputs (`lerp`, `clamp`, `map`) are left out, and `Output Vector`,
`Clamp Output`.

| Input | Type | Default | Range |
|---|---|---|---|
| `type` | enum `add`, `subtract`, `multiply`, `divide`, `power`, `root`, `sin`, `cos`, `tan`, `modulo`, `floor`, `ceil`, `round`, `abs`, `fract`, `snap`, `log`, `max`, `min` | `add` | |
| `a` | number | 0 | |
| `b` | number | 0 | |
| `angle` | enum `radians`, `degree` | `degree` | |
| `to_integer` | bool | false | |

| Output | Type |
|---|---|
| `result` | number |

`root` = `a^(1/b)`; `modulo` = `a − b·trunc(a/b)`; `snap` = `round(a/b)·b`; `log` = `ln a / ln b`; trig takes `a` in
`angle` units. Deviation: `divide`, `modulo`, `snap` by 0 give 0, `a`, `a`. A non-finite result is an `eval_error`.
`to_integer` rounds.

## counter — value

Upstream: `Node_Counter` ("Frame Index"). Left out: `Async`, `Reset`. Reads the frame.

| Input | Type | Default | Range |
|---|---|---|---|
| `mode` | enum `frame_count`, `animation_progress` | `frame_count` | |
| `start` | number | 1 | |
| `speed` | number | 1 | |

| Output | Type |
|---|---|
| `value` | number |

`frame_count`: `start + frame · speed`; `animation_progress`: `frame / (frames − 1) · speed` (0 when `frames = 1`).

### Value node tests

| ID | Given | Expected |
|---|---|---|
| N-VALUE-01 | `number` 2.5 | `Number(2.5)` |
| N-VALUE-02 | `color` `#123456` feeding `solid.color` | solid is (0x12, 0x34, 0x56, 255) |
| N-VALUE-03 | `palette` `["#ff0000", "#00ff00"]` feeding `posterize.palette`, red-ish input | output red |
| N-VALUE-04 | `math` add 2, 3; divide 1, 0; sin 90 (degree); power 2, 10; modulo -7, 3; snap 7, 5; log 8, 2 | 5; 0; 1; 1024; -1; 5; 3 |
| N-VALUE-05 | `math` power -1, 0.5 (NaN) | `eval_error` |
| N-VALUE-06 | `counter` default at frame 3 of 10; `animation_progress` at frame 9 of 10, speed 2 | 4; 2 |
| N-VALUE-07 | `counter` evaluated at frames 0 and 1 in one evaluator | re-runs (frame is part of the key) and gives 1 then 2 |
