# Nodes

Behavior of each node type. The catalog entry (see [types.md](types.md)) is generated from the tables here.
Size rule: generators have an optional `size` input; absent, they produce the project's `width × height`.
Other nodes state their size.

M1 nodes: `solid`, `noise`, `transform`, `blend`, `image`, `output`. The remaining nodes are specified in M3.

## Shared definitions

`hash(u32) → u32` is the PCG hash:

```
state = v * 747796405u + 2891336453u
word  = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u
hash  = (word >> 22u) ^ word
```

All arithmetic wraps modulo 2³². `hash2(x, y, seed) = hash(x ^ hash(y ^ hash(seed)))`, with `x`, `y`, `seed` cast to `u32`
(two's complement for negatives). `unit(h) = h / 2³²`, a float in `[0, 1)`.

## solid — generate

| Input | Type | Default | Range |
|---|---|---|---|
| `color` | color | `#000000` | |
| `size` | size | project size | |

| Output | Type |
|---|---|
| `out` | surface, `size` |

Every pixel is `color`.

| ID | Given | Expected |
|---|---|---|
| N-SOLID-01 | 4×3 project, `color` `#ff800040` | 4×3 surface, every pixel (255,128,0,64) |
| N-SOLID-02 | no inputs | every pixel (0,0,0,255) |
| N-SOLID-03 | 4×3 project, `size [2, 5]` | 2×5 surface |

## noise — generate

| Input | Type | Default | Range |
|---|---|---|---|
| `kind` | enum `white`, `value` | `value` | |
| `seed` | int | 0 | |
| `scale` | int | 4 | 1–1024 |
| `size` | size | project size | |

| Output | Type |
|---|---|
| `out` | surface, `size` |

Grey, opaque: pixel = (g, g, g, 255).

- `white`: `g = floor(unit(hash2(x, y, seed)) × 256)`. `scale` is ignored.
- `value`: lattice points every `scale` pixels. For pixel `(x, y)`: `cx = floor(x / scale)`, `fx = (x mod scale) / scale`
  (same for y). Corner values `v(i, j) = unit(hash2(i, j, seed))`. `t = smoothstep(f) = f²(3 − 2f)`.
  `v = bilinear(v(cx,cy), v(cx+1,cy), v(cx,cy+1), v(cx+1,cy+1); tx, ty)`, `g = floor(v × 256)`, clamped to 255.

The shader computes `value` in `f32`; the golden image check for `value` allows a difference of 1.

| ID | Given | Expected |
|---|---|---|
| N-NOISE-01 | `white`, seed 0, 4×4 | equals golden `noise_white_s0.png` exactly; pixel (0,0) equals the formula computed on the CPU in the test |
| N-NOISE-02 | `white`, seeds 0 and 1 | outputs differ |
| N-NOISE-03 | `white`, 64×64 | mean of g in 112–144; all 4096 pixels opaque and grey |
| N-NOISE-04 | `value`, `scale` 4, 16×16 | equals golden `noise_value_sc4.png` within 1; pixels with `x mod 4 = 0` and `y mod 4 = 0` equal `floor(unit(hash2(x/4, y/4, 0)) × 256)` |
| N-NOISE-05 | `value`, `scale` 1 | equals `white` with the same seed |
| N-NOISE-06 | `scale` 0 | load error `out_of_range` |
| N-NOISE-07 | `white`, seed 0, `size [8, 8]` in a 4×4 project | 8×8; its top-left 4×4 equals N-NOISE-01 (noise depends on pixel position, not size) |

## transform — transform

| Input | Type | Default | Range |
|---|---|---|---|
| `image` | surface | required | |
| `offset` | vec2 | `[0, 0]` | pixels |
| `rotation` | number | 0 | degrees, clockwise on screen |
| `scale` | vec2 | `[1, 1]` | each component ≠ 0 at evaluation, else `eval_error` |
| `pivot` | vec2 | `[0.5, 0.5]` | fraction of the input size |
| `wrap` | bool | false | |

| Output | Type |
|---|---|
| `out` | surface, same size as `image` |

For output pixel centre `p = (x+0.5, y+0.5)`, with `c = pivot × size`:
`q = c + S⁻¹ · R⁻¹ · (p − c − offset)`, where `R` rotates by `rotation` clockwise (y down) and `S = diag(scale)`.
The output pixel is input pixel `floor(q)`; outside the input it is `(0,0,0,0)`, or with `wrap` it is
`floor(q) mod size` (always non-negative).

Rotation by a multiple of 90° and integer offsets must be exact. Implementations snap `sin`/`cos` of multiples of 90° to
exact 0/±1.

| ID | Given | Expected |
|---|---|---|
| N-TRANSFORM-01 | 4×4 asymmetric fixture, all defaults | output equals input |
| N-TRANSFORM-02 | fixture, `offset [1, 0]` | column 0 transparent; column x equals input column x−1 |
| N-TRANSFORM-03 | fixture, `offset [1, 0]`, `wrap` | column 0 equals input column 3 |
| N-TRANSFORM-04 | fixture, `rotation 90` | output (x, y) equals input (y, 3−x) (clockwise) |
| N-TRANSFORM-05 | fixture, `rotation 360` and `-360` | equal input |
| N-TRANSFORM-06 | 2×2 fixture in a 4×4 surface at top-left, `scale [2, 2]`, `pivot [0, 0]` | each input pixel becomes a 2×2 block |
| N-TRANSFORM-07 | fixture, `scale [-1, 1]` | horizontal mirror |
| N-TRANSFORM-08 | `scale [0, 1]` | `eval_error` on the transform node |
| N-TRANSFORM-09 | 5×3 fixture, `rotation 90` | output 5×3, golden `transform_rot90_5x3.png` exactly |

## blend — compose

| Input | Type | Default | Range |
|---|---|---|---|
| `background` | surface | required | |
| `foreground` | surface | required | |
| `mode` | enum `normal`, `add`, `multiply`, `screen` | `normal` | |
| `opacity` | number | 1 | 0–1 |

| Output | Type |
|---|---|
| `out` | surface, same size as `background` |

The foreground's top-left sits on the background's top-left. Outside the foreground, the output is the background.
Inside, with straight-alpha colours in 0–1 (`Cb, ab` background; `Cs, as` foreground, `as` multiplied by `opacity`),
following W3C Compositing and Blending Level 1:

```
B(Cb, Cs): normal = Cs; add = min(Cb + Cs, 1); multiply = Cb·Cs; screen = Cb + Cs − Cb·Cs
Cs' = (1 − ab)·Cs + ab·B(Cb, Cs)
ao  = as + ab·(1 − as)
co  = as·Cs' + ab·Cb·(1 − as)
out = ao > 0 ? (co / ao, ao) : (0, 0, 0, 0)
```

| ID | Given | Expected |
|---|---|---|
| N-BLEND-01 | opaque red over opaque blue, `normal` | red |
| N-BLEND-02 | opaque white over opaque black, `opacity 0.5` | (128,128,128,255) ± 1 |
| N-BLEND-03 | fully transparent foreground | output equals background |
| N-BLEND-04 | both fully transparent | (0,0,0,0) |
| N-BLEND-05 | opaque (128,128,128) over opaque (255,0,64), `multiply` | (128,0,32,255) ± 1 |
| N-BLEND-06 | opaque (200,100,0) + opaque (100,200,50), `add` | (255,255,50,255) |
| N-BLEND-07 | opaque (128,0,255) over opaque (128,255,0), `screen` | (192,255,255,255) ± 1 |
| N-BLEND-08 | opaque red over transparent background, `multiply` | opaque red (blend mode has no effect where `ab = 0`) |
| N-BLEND-09 | 2×2 foreground on 4×4 background | only the top-left 2×2 changes |
| N-BLEND-10 | 8×8 foreground on 4×4 background | output 4×4 |

## image — io

| Input | Type | Default | Range |
|---|---|---|---|
| `path` | path | required | |

| Output | Type |
|---|---|
| `out` | surface, the file's size |

Reads a PNG file. Any PNG colour type and bit depth is converted to RGBA8 (16-bit channels are rounded down to 8 bits by
taking the high byte). Colour profiles and gamma chunks are ignored. Other formats fail with `eval_error`.

| ID | Given | Expected |
|---|---|---|
| N-IMAGE-01 | RGBA8 PNG fixture | surface bytes equal the PNG's pixel bytes |
| N-IMAGE-02 | palette PNG with a transparent entry, greyscale PNG, 16-bit RGB PNG | converted as described; equal to golden RGBA bytes |
| N-IMAGE-03 | path relative to the project file, project loaded from another working directory | file found |
| N-IMAGE-04 | missing file | `eval_error` naming the path |
| N-IMAGE-05 | a JPEG file renamed to `.png` | `eval_error` |
| N-IMAGE-06 | 5000×1 PNG | `eval_error` (over size limit) |

## output — io

| Input | Type | Default | Range |
|---|---|---|---|
| `image` | surface | required | |

| Output | Type |
|---|---|
| `out` | surface, equal to `image` |

Marks what `slopc render` writes when no `--node` is given (see [cli.md](cli.md)).

| ID | Given | Expected |
|---|---|---|
| N-OUTPUT-01 | `image` = ref to a solid | output equals the solid |
