# Project file

A project is one UTF-8 JSON file named `*.slopc.json`. It is the source of truth:
the CLI, MCP and the server read it from disk and write it back.

## Format

```json
{
  "slopc": 1,
  "settings": { "width": 32, "height": 32, "frames": 1, "fps": 12 },
  "nodes": {
    "bg":  { "type": "solid", "inputs": { "color": "#1a1c2c" } },
    "out": { "type": "output", "inputs": { "surface_in": { "ref": "bg" } }, "pos": [240, 0] }
  }
}
```

### Top level

| Key | Type | Required | Rule |
|---|---|---|---|
| `slopc` | integer | yes | format version, must be `1` |
| `settings` | object | no | missing keys take their defaults |
| `nodes` | object | no | node id → node; missing means no nodes |

Unknown keys at any level are errors. Strictness makes typos visible, which matters for AI authors.

### Settings

| Key | Type | Default | Range |
|---|---|---|---|
| `width` | integer | 32 | 1–4096 |
| `height` | integer | 32 | 1–4096 |
| `frames` | integer | 1 | 1–10000 |
| `fps` | number | 12 | > 0, ≤ 240 |

`width × height` must be ≤ 4,194,304 (2048²).

### Node

| Key | Type | Required | Rule |
|---|---|---|---|
| `type` | string | yes | a type from the catalog |
| `inputs` | object | no | input name → input value; missing inputs take their default |
| `label` | string | no | free note for people and models, at most 500 characters; ignored by evaluation |
| `pos` | `[number, number]` | no | editor position; ignored by evaluation |

Node ids match `^[a-z][a-z0-9_]{0,63}$`. A project has at most 1000 nodes.

### Input value

An input holds either a **literal** of the input's type (see [types.md](types.md)) or a **ref**:

```json
{ "ref": "noise" }
{ "ref": "noise.surface_out" }
```

- `"node"` refers to the node's first output, `"node.output"` to a named output.
- A ref object has exactly the key `ref`.
- Inputs of type `surface` accept only refs.
- Other inputs may also hold an animation `{"anim": [...], "on_end": ...}` ([animation.md](animation.md)).

## Load

Load = parse, then validate. Validation collects **all** errors; it does not stop at the first.

Each error is:

```json
{ "code": "dangling_ref", "node": "quant", "input": "surface_in", "message": "node 'quant' input 'surface_in' refers to missing node 'nose'" }
```

`node` and `input` are present when the error belongs to them. `message` is for people; `code` is stable.

| Code | When |
|---|---|
| `parse_error` | not valid JSON, or the wrong JSON type at a fixed key (e.g. `nodes` is an array) |
| `unsupported_version` | `slopc` is missing or not `1` |
| `unknown_key` | a key not defined in this spec |
| `invalid_id` | a node id breaks the id pattern |
| `unknown_type` | `type` is not in the catalog |
| `unknown_input` | an input name the node type does not have |
| `missing_input` | a required input is absent |
| `type_mismatch` | a literal of the wrong type, a literal on a `surface` input, or a ref whose output type does not fit |
| `out_of_range` | a number outside its range, a settings value outside its range |
| `invalid_enum` | an enum string not in its options |
| `dangling_ref` | a ref to a node id that does not exist |
| `unknown_output` | a ref to an output name the node does not have |
| `cycle` | refs form a cycle; reported once per node in the cycle |
| `too_large` | more than 1000 nodes, or `width × height` (settings or a `dimension` input) over the cap |

A `parse_error` or `unsupported_version` stops validation, since nothing else can be checked.
An `unknown_type` node is skipped for input checks; refs to it are still checked for existence.

## Save

- The file is written as canonical JSON: 2-space indent, `\n` line endings, trailing newline,
  keys in the order of this spec, nodes and inputs in their existing order.
- Save writes exactly the inputs the model holds: it never adds defaults, and never drops a value equal to its default.
- The write is atomic: write `<file>.tmp-<random>` in the same directory, fsync, rename over the file.
- Writers hold an exclusive advisory lock on the sidecar file `<file>.lock` for the whole read-modify-write.
  The lock is on a sidecar because rename replaces the project file's inode.
- Readers do not lock; atomic rename means they see the old or the new file, never a mix.
- An invalid project is never saved, with one exception: `missing_input` errors are allowed, so a graph can be built
  step by step (upstream also allows unconnected inputs). Evaluating a node whose dependencies include a missing
  required input fails with that `missing_input` error.

## Test cases

| ID | Given | Expected |
|---|---|---|
| PF-01 | `{"slopc":1}` | valid; settings 32×32, 1 frame, 12 fps; no nodes |
| PF-02 | the example in this file | valid; 2 nodes; `out.surface_in` is a ref to `bg` output `surface_out` (first output) |
| PF-03 | `not json` | one error `parse_error` |
| PF-04 | `{"slopc":2}` | one error `unsupported_version` |
| PF-05 | `{}` | one error `unsupported_version` |
| PF-06 | `{"slopc":1,"extra":true}` | `unknown_key` |
| PF-07 | node with key `note` | `unknown_key` with `node` set |
| PF-08 | node id `Noise` and node id `1a` | two `invalid_id` errors |
| PF-09 | node `type: "nope"` | `unknown_type`, no input errors for that node |
| PF-10 | `solid` with input `colour` | `unknown_input` with `node`, `input` |
| PF-11 | `output` without `surface_in` | `missing_input` |
| PF-12 | `solid.color` = `5` | `type_mismatch` |
| PF-13 | `output.surface_in` = `"#ffffff"` (literal on a surface) | `type_mismatch` |
| PF-14 | `transform.position` = ref to a `solid` (surface → vec2) | `type_mismatch` |
| PF-15 | `blend.opacity` = `1.5` | `out_of_range` |
| PF-16 | `settings.width` = `0`, and `5000` | `out_of_range` each |
| PF-17 | `settings` 4096×4096 | `too_large` |
| PF-18 | 1001 nodes | `too_large` |
| PF-19 | `blend.blend_mode` = `"darken"` | `invalid_enum` |
| PF-20 | ref `{"ref":"nose"}` | `dangling_ref` |
| PF-21 | ref `{"ref":"bg.alpha"}` on a `solid` | `unknown_output` |
| PF-22 | `a` refs `b`, `b` refs `a` | `cycle` for `a` and for `b` |
| PF-23 | node refs itself | `cycle` |
| PF-24 | ref object `{"ref":"bg","x":1}` | `unknown_key` |
| PF-25 | a file with PF-10, PF-12 and PF-20 together | all three errors reported |
| PF-26 | ref to an `unknown_type` node | no `dangling_ref`; only the `unknown_type` error |
| PF-27 | load PF-02, save, load again | identical model; saved bytes equal the canonical form; saving twice gives identical bytes |
| PF-28 | file with `"color": "#000000"` written explicitly (the default) | after save, the key is still present |
| PF-29 | save of an invalid model | refused with the validation errors; file on disk unchanged |
| PF-30 | another process holds `<file>.lock` | save waits until it is released, then succeeds |
| PF-31 | save is interrupted after the temp write (simulated by failing the rename) | original file unchanged; temp file removed |
| PF-34 | save of a model whose only error is `missing_input` | saved; loading reports the `missing_input` error |
| PF-32 | node with `"label": "sky gradient"` | valid; kept through save; evaluation output equal to the same node without it |
| PF-33 | `label` = `5`, and a 501-character string | `type_mismatch`, `out_of_range` |
