# Edit operations

The one edit vocabulary. `slopc edit`, the MCP tool `edit_project` and the server's HTTP API all apply the same ops.

## Ops

An op is a JSON object with an `op` field. Unknown fields are errors (`bad_op`).

| `op` | Fields | Effect |
|---|---|---|
| `add_node` | `type`, optional `id`, `inputs`, `pos`, `label` | Adds a node at the end. Without `id`, the id is the type plus the smallest positive integer not in use (`solid1`, `solid2`, …; upstream also names new nodes after their type). |
| `remove_node` | `id` | Removes the node. Inputs of other nodes that ref it are unset (upstream disconnects them). |
| `rename_node` | `id`, `new_id` | Renames; every ref to it (`id` and `id.output`) is rewritten. Order is kept. |
| `move_node` | `id`, `pos` | Sets `pos` (`[number, number]`). |
| `set_input` | `id`, `input`, `value` | Sets an input to a literal or `{"ref": …}`. |
| `unset_input` | `id`, `input` | Removes the input (it takes its default). Unsetting an unset input is not an error. |
| `set_label` | `id`, `label` | Sets the label; `null` removes it. |
| `set_settings` | any of `width`, `height`, `frames`, `fps` | Sets those settings. |

## Batches

`apply(project, ops) → result` applies a list of ops in order to a copy of the project.

- An op that cannot apply (unknown node, duplicate id, malformed op) rejects the batch with that error, which carries
  `op` = the op's index.
- After the last op, the project is validated. If it has errors other than `missing_input`, the batch is rejected with
  them. Otherwise it succeeds; remaining `missing_input` errors are returned as `warnings`.
- A rejected batch changes nothing.
- On a file, the batch runs under the project lock: load, apply, save. A file that cannot be parsed cannot be edited.

Result:

```json
{ "ok": true, "created": ["solid1"], "warnings": [ { "code": "missing_input", "node": "b", "input": "background", "message": "..." } ] }
{ "ok": false, "errors": [ { "code": "duplicate_id", "op": 0, "node": "bg", "message": "..." } ] }
```

Op error codes: `bad_op` (unknown `op`, missing or wrong field, unknown field), `unknown_node`, `duplicate_id`,
`invalid_id`, `unknown_type`, `unknown_input`; plus every validation code from [project-file.md](project-file.md).

## Test cases

| ID | Given | Expected |
|---|---|---|
| OP-01 | empty project, `add_node {type: solid}` twice | ids `solid1`, `solid2`; `created` lists them |
| OP-02 | `add_node {id: bg, type: solid, inputs: {color: "#ff0000"}, pos: [1, 2], label: "x"}` | node as given |
| OP-03 | `add_node` with an existing id | rejected, `duplicate_id`, `op` 0 |
| OP-04 | `add_node {type: nope}` | rejected, `unknown_type` |
| OP-05 | `add_node {id: "Bad"}` | rejected, `invalid_id` |
| OP-06 | `remove_node bg` where `out.surface_in` refs `bg` | `bg` gone; `out` has no `surface_in`; warning `missing_input` |
| OP-07 | `rename_node bg → sky` with refs `bg` and `bg.surface_out` | refs become `sky` and `sky.surface_out`; node order kept |
| OP-08 | `rename_node` to an existing id | rejected, `duplicate_id` |
| OP-09 | `move_node bg [10, 20]` | `pos` is `[10, 20]` |
| OP-10 | `set_input bg color "#00ff00"` | input set |
| OP-11 | `set_input bg color 5` | rejected, `type_mismatch` with `node` and `input` |
| OP-12 | `set_input out surface_in {ref: bg}` | ref set |
| OP-13 | `set_input bg nope 1` | rejected, `unknown_input` |
| OP-14 | `unset_input bg color` | input removed; repeating it succeeds |
| OP-15 | `set_label bg "sky"` then `set_label bg null` | label set, then removed |
| OP-16 | `set_settings {width: 64}` | width 64, others unchanged |
| OP-17 | `set_settings {width: 0}` | rejected, `out_of_range` |
| OP-18 | `{op: "explode"}`, `{op: "move_node"}` (no id), `{op: "move_node", id: bg, pos: [1,2], x: 1}` | each rejected, `bad_op` |
| OP-19 | batch `[add ok, add duplicate]` | rejected with `op` 1; project unchanged |
| OP-20 | batch creating a cycle | rejected, `cycle` |
| OP-21 | ops on an unknown node id | rejected, `unknown_node` |
| OP-22 | batch `[add blend b]` (missing background) | ok; warning `missing_input` |
| OP-23 | `slopc edit a.slopc.json ops.json` with a valid batch | exit 0; file updated |
| OP-24 | `slopc edit` with a rejected batch, `--json` | exit 1; `{ok: false, errors}`; file unchanged |
| OP-25 | `slopc edit a.slopc.json -` with ops on stdin | applied |
| OP-26 | `slopc edit` input that is a single op object, not an array | applied as a batch of one |
| OP-27 | 8 concurrent `slopc edit` processes each adding a node | all 8 nodes present (lock serialises) |
