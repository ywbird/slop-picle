# Evaluation and GPU

## Evaluation

`evaluate(project, target, frame) → value | error`

- Only valid projects are evaluated, except for `missing_input` errors outside the target's dependencies
  (see [project-file.md](project-file.md#save)); one inside them is returned as the evaluation error.
- `target` is a node id, optionally with `.output`. Only the target and the nodes it depends on are evaluated.
- `frame` is an integer, `0 ≤ frame < settings.frames`; otherwise the error `frame_out_of_range`.
- Order: dependencies before dependents. Evaluation is deterministic: the same project, target and frame give the same bytes
  on the same machine.
- A missing optional input takes its catalog default; a missing `dimension` input takes the project's `[width, height]`.
- A ref value outside its input's range is clamped to the range at evaluation (it was not checkable at load).
- If a node fails, evaluation stops with:

```json
{ "code": "eval_error", "node": "img", "message": "cannot read 'assets/hero.png': no such file" }
```

### Cache

Within one process, node outputs are cached. The cache key of a node is a hash of:
its type, its literal inputs (after defaults), the cache keys of its ref inputs, the frame if the node reads the frame,
and the content of files it reads (`image`).

- Evaluating the same target twice runs no node the second time.
- Changing one input re-runs that node and its dependents only.
- The cache is bounded (default 256 MiB of surfaces); least recently used entries are dropped.

## Surfaces

- A surface is a 2D image, RGBA, 8 bits per channel, **straight** (not premultiplied) alpha, no colour management:
  the bytes in a PNG are the bytes in the texture.
- Coordinates: origin top-left, `x` right, `y` down. Pixel `(x, y)` covers `[x, x+1) × [y, y+1)`; its centre is `(x+0.5, y+0.5)`.
- Sampling is nearest: a point `p` maps to pixel `floor(p)`. No filtering anywhere in M1.
- Reading outside a surface gives `(0,0,0,0)` unless a node defines wrapping.
- Width and height are each 1–4096 and their product ≤ 2048²; a node producing anything else fails with `eval_error`.

## GPU

- Rendering uses `wgpu` with WGSL shaders, headless (no window, no display server).
- Backends: Vulkan, Metal, DX12; GL is not used. The adapter is the first one wgpu offers with default power preference;
  `SLOPC_ADAPTER=<substring>` picks the first adapter whose name contains the substring.
- If no adapter is found, commands that render fail with `gpu_unavailable` (CLI exit code 4, see [cli.md](cli.md)).
  Commands that do not render (`validate`, `nodes`, ...) never open the GPU.
- Textures are `Rgba8Unorm`. Shaders read texels with `textureLoad` at integer coordinates, not samplers.
- A shader node is: one WGSL function `fn shade(px: vec2<i32>) -> vec4<f32>` + its catalog entry. The prelude
  (uniform struct, texture bindings, helpers) is generated from the catalog inputs. Each non-surface input takes one
  16-byte field named after it: number → `vec4<f32>` (`.x`), int → `vec4<i32>` (`.x`), bool and enum index → `vec4<u32>` (`.x`),
  color → `vec4<f32>` in 0–1, vec2 → `vec4<f32>` (`.xy`), dimension → `vec4<u32>` (`.xy`). Strings and paths are not passed.
  Each surface input `s` gets `s_info: vec4<u32>` (width, height, present) and a helper `load_s(p: vec2<i32>)` that returns
  `(0,0,0,0)` outside the surface. `out_info: vec4<u32>` holds (width, height, frame). A node may add extra `vec4<f32>`
  fields computed on the CPU (e.g. snapped `cos`/`sin`).
- Float to byte: `round(clamp(v, 0, 1) × 255)`, which is what `Rgba8Unorm` storage does.
- Random numbers come from an integer hash in `u32` arithmetic (see `noise` in [nodes.md](nodes.md)),
  never from float tricks such as `fract(sin(x))`, so all GPUs give the same bits.

## Golden images

- Nodes whose result is integer-exact (solid, `greyscale` noise, transform without `alpha`, image, output) must match golden images exactly.
- Float math (`rgb`/`hsv` noise, transform `alpha`, blend) may differ by at most 1 per channel.

## Test cases

| ID | Given | Expected |
|---|---|---|
| EV-01 | target = a node id | that node's first output |
| EV-02 | target = `node.output` with an unknown output | error `unknown_output` (same code as load) |
| EV-03 | target = an unknown node | error `dangling_ref` |
| EV-04 | `frame` = `settings.frames` and `-1` | `frame_out_of_range` each |
| EV-05 | a project with a node unrelated to the target that would fail (`image` with a missing file) | target evaluates fine |
| EV-06 | `image` with a missing file as a dependency | `eval_error` with `node` = that image node |
| EV-07 | evaluate the same target twice in one process | second run executes 0 nodes (counted by an instrumented runner) |
| EV-08 | chain a→b→c, change an input of b, evaluate c | a not re-run; b and c re-run |
| EV-09 | `image` file content changes between evaluations (same path) | image node and dependents re-run |
| EV-11 | a project with an unconnected `blend` (missing `background`) and a separate solid target | target evaluates fine |
| EV-12 | target depends on a node with a missing required input | error `missing_input` naming that node |
| EV-10 | ref value out of range (an M3 `number` node output of 5 into `blend.opacity`) | clamped to 1 (test added in M3) |
| GPU-01 | `SLOPC_ADAPTER=no-such-gpu` and render | `gpu_unavailable` |
| GPU-02 | `validate` with `SLOPC_ADAPTER=no-such-gpu` | succeeds; the GPU is never opened |
| GPU-03 | upload a 3×2 surface with distinct pixels and read it back | bytes identical, including alpha 0 pixels with non-zero RGB |
| GPU-04 | same project rendered twice in separate processes | identical PNG bytes |
