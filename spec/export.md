# Export

Upstream: Pixel Composer `scripts/node_export` (`Node_Export`) and `scripts/node_render_sprite_sheet`
(`Node_Render_Sprite_Sheet`). By the earlier decision, a slopc project has no Export node: the export command (CLI,
MCP, server, web UI) picks the node, the frames and the file. The sprite sheet is a node, as upstream.

## Export

`export(project, node, out, options)` renders frames of a surface node and writes files.

| Option | Default | Rule |
|---|---|---|
| `node` | the single `output` node | as `slopc render` (`no_target`, `ambiguous_target`) |
| `out` | required | file name template, see below; the extension picks the format |
| `range` | all frames | `[a, b]`, 1-based and inclusive (upstream "Frame range"); `a ≥ 1`, `a ≤ b ≤ frames`, else `out_of_range` |
| `step` | 1 | ≥ 1 (upstream "Frame step"): frames `a − 1`, `a − 1 + step`, … ≤ `b − 1` |
| `begin` | 0 | added to the number written by `%f` (upstream "Sequence begin") |
| `scale` | 1 | integer 1–32, nearest-neighbour (upstream "Scale"; integer only, for pixel art) |
| `fps` | project fps | > 0; frame time of animations (upstream "Framerate" in project units) |
| `frame` | 0 | the frame of a single image |

### Template and format

`out` follows upstream's template: `%f` is replaced by the frame number `f + 1 + begin` (frames count from 1, as
upstream), `%Nf` pads it with zeros to `N` digits (`%3f` → `007`). `%%` is a literal `%`. Other `%` sequences are
`bad_path` (upstream's `%d`, `%n`, `%i`, `%s`, `%r` have no meaning without an Export node).

| Extension | Upstream type | Result |
|---|---|---|
| `.png` with `%f` | Image sequence | one PNG per exported frame |
| `.png` without `%f` | Single image | one PNG of `frame` (`range`/`step` are ignored) |
| `.gif` | Animation | one GIF |
| `.apng` | Animation | one animated PNG |

Any other extension, or `%f` in a `.gif`/`.apng` name, is `bad_path`. JPG, WebP, EXR, BMP, ICO, MP4 and WebM are left out
(upstream needs ffmpeg or ImageMagick for most of them).

**GIF.** One image per frame with delay `max(1, round(100 / fps))` centiseconds (upstream builtin encoder:
`100 / fps`), disposal "restore to background" (upstream `-dispose 2`) and an infinite loop (upstream "Loop" on).
Pixels with alpha < 128 are transparent; other pixels are opaque. If a frame has at most 255 distinct opaque colours,
its palette is exact; otherwise it is quantised (NeuQuant), which changes colours.

**APNG.** Lossless RGBA; `num_plays = 0` (infinite); each frame's delay is `1/fps` s (`1/fps` exactly for whole fps,
else `round(1000/fps)/1000`).

Files are written atomically (temp file and rename); missing parent directories are created. GIF and APNG frames must
all have the size of the first, else `eval_error`; GIF frames are at most 65535 px per side after scaling, else
`out_of_range`. The result lists the written paths in order and the frames used.

## Render Spritesheet (node)

Type `render_sprite_sheet`, category `compose`. Upstream "Sprite Set" `Sprite array`, "Atlas Data" output and array
inputs are left out (no arrays yet).

| Input | Type | Default | Range |
|---|---|---|---|
| `sprites` | surface | required | |
| `frame_step` | int | 1 | ≥ 1 |
| `skip_empty` | bool | false | |
| `packing_type` | enum `horizontal`, `vertical`, `grid` | `horizontal` | |
| `grid_column` | int | 4 | ≥ 1 |
| `alignment` | enum `first`, `middle`, `last` | `first` | |
| `spacing` | int | 0 | ≥ 0 (horizontal and vertical packing) |
| `grid_spacing` | vec2 | `[0, 0]` | each ≥ 0 (grid packing; upstream names both inputs "Spacing") |
| `padding` | padding | `[0, 0, 0, 0]` | right, top, left, bottom |
| `custom_range` | bool | false | |
| `range` | vec2 | `[0, 0]` | used when `custom_range`: start and end, 1-based and inclusive; 0 means first / last; negative counts from the end (`-1` is the last frame) |

| Output | Type |
|---|---|
| `surface_out` | surface |

Frames used: from `start` to `end` (0-based) every `frame_step`; with `skip_empty`, fully transparent frames are left out
(they take no cell). The node evaluates `sprites` at each of those frames (its key hashes the source's keys at those
frames and the frame count, as time remapping does; it does not depend on the current frame). Cells are `W × H`, the
largest width and height among the frames. With `n` frames:

- `horizontal`: frame `i` at `x = left + i·(W + spacing)`, `y = top`; size `(n·W + (n − 1)·spacing + left + right,
  H + top + bottom)`.
- `vertical`: `x = left`, `y = top + i·(H + spacing)`; size `(W + left + right, n·H + (n − 1)·spacing + top + bottom)`.
- `grid` with `c = grid_column` columns and `r = ceil(n / c)` rows: frame `i` at column `i mod c`, row `floor(i / c)`,
  `x = left + col·(W + gx)`, `y = top + row·(H + gy)`; size `(c·W + (c − 1)·gx + left + right, r·H + (r − 1)·gy + top +
  bottom)`.

A frame smaller than its cell is placed by `alignment` along the cross axis (horizontal: top / middle / bottom;
vertical: left / middle / right; grid: top-left always, as upstream), `middle` rounding down. No frames (all empty, or
an empty range) give a 1×1 transparent surface; a sheet over the surface size limits is an `eval_error`.

Deviation: upstream has no single rule here. It computes the range one way when it sizes the sheet (`animationInit`:
negative start `frames − 2 + a`, exclusive end `b`) and another way when it draws (`animationRender`: inclusive end
`b + 1`), sizes for `floor((end − start)/step)` cells so the last frame can be clipped, places frames by their own size
rather than a common cell, and aligns against the whole sheet including padding. slopc uses the one rule above for
sizing and drawing, counts every used frame and aligns inside the cell.

## CLI

`slopc export <file> -o <out> [--node ID] [--range A:B] [--step N] [--begin N] [--scale K] [--fps F] [--frame N] [--json]`

Exit codes as [cli.md](cli.md): 1 invalid project, evaluation error or `out_of_range`; 2 bad command line or `bad_path`;
3 I/O; 4 no GPU. `--json` prints `{ "ok": true, "files": [...], "frames": [...] }`.

## MCP and server

- MCP tool `export`: `{ path, out, node?, range?, step?, begin?, scale?, fps?, frame? }`; `out` is relative to the root and
  must stay inside it (root sandbox). Returns `{ files, frames }`.
- `POST /api/export?path=` with the same JSON (without `path`) writes into the root and returns `{ ok, files, frames }`.
- `GET /api/file?path=` serves a `.png`, `.gif` or `.apng` file inside the root (`image/png`, `image/gif`,
  `image/apng`), with `Content-Disposition: attachment`; other extensions are `bad_path`, missing files `not_found`.

## Web UI

An Export button in the top bar opens a dialog: format (GIF, APNG, PNG sequence, PNG image), node (the selected node if
it outputs a surface, else the output node), range, step, scale, fps, and the output path (default
`exports/<project name>.gif`, `.apng`, `_%3f.png` or `.png`, editable). Export calls `POST /api/export` and lists the
written files as download links (`/api/file`). Errors show in the dialog.

## Test cases

| ID | Given | Expected |
|---|---|---|
| EX-01 | 4 frames, solid with a cut-keyed colour per frame; `-o seq_%f.png` | `seq_1.png` … `seq_4.png`, each with its frame's colour |
| EX-02 | `-o seq_%3f.png --begin 10`; `-o a%%b_%f.png` | `seq_011.png` … `seq_014.png`; `a%b_1.png` … |
| EX-03 | 6 frames: `--range 2:3`; `--step 2`; `--range 0:3`, `--range 3:2`, `--range 1:7` | frames 1, 2 (files `_2`, `_3`); frames 0, 2, 4; `out_of_range` (exit 1) each |
| EX-04 | `-o a.gif --fps 4`, 4 frames, one transparent pixel | a GIF with 4 frames, delay 25, infinite loop, disposal background; colours exact; the transparent pixel transparent |
| EX-05 | `-o a.apng --fps 12` | an APNG with 4 frames, `num_plays` 0, delay 1/12, pixels and alpha exact |
| EX-06 | `--scale 3` on a 4×4 project | 12×12 frames |
| EX-07 | `-o a.png --frame 2` | one file, frame 2's picture |
| EX-08 | `-o a.jpg`; `-o a_%f.gif`; `-o a_%x.png` | `bad_path`, exit 2 |
| EX-09 | a GIF of a 32×32 noise in RGB mode (> 255 colours) | written; 1 frame |
| EX-10 | `--json` | `{ ok, files, frames }` with the paths in order |
| N-SHEET-01 | 4 frames of 3×2, defaults | 12×2; cell `i` holds frame `i` |
| N-SHEET-02 | `vertical`, `spacing 1` | 3×11; frame 1 at y = 3 |
| N-SHEET-03 | `grid`, `grid_column 3`, `grid_spacing [1, 2]`, 5 frames | 11×6; frame 4 at (4, 4) |
| N-SHEET-04 | `padding [1, 2, 3, 4]` (horizontal, 4 frames 3×2) | 16×8; frame 0 at (3, 2) |
| N-SHEET-05 | `frame_step 2` over 6 frames; `custom_range [2, -2]` over 6 frames | frames 0, 2, 4; frames 1 … 4 |
| N-SHEET-06 | `skip_empty` where frames 1 and 2 are transparent (of 4) | 2 cells: frames 0 and 3 |
| N-SHEET-07 | frames of animated size 2×2 and 4×4, `alignment middle` (horizontal) | cell 4×4; the 2×2 frame at y = 1 |
| N-SHEET-08 | one evaluator: sheet of 4 frames, then the source's key at frame 3 changes | the sheet's last cell changes |
| MCP-14 | `export` with `out: "exports/a.gif"`; `out: "../a.gif"` | file written, listed; `path_outside_root` |
| SRV-23 | `POST /api/export` a PNG sequence, then `GET /api/file` one of them; `GET /api/file?path=a.slopc.json` | files written, 200 `image/png` attachment equal to the file; 400 `bad_path` |
| WEB-29 | headless Chromium: Export dialog, GIF, Export | the GIF exists in the root; its download link returns a GIF |
