# slopc

Node-based procedural pixel-art compositor inspired by [Pixel Composer](https://github.com/Ttanasart-pt/Pixel-Composer).
Projects are `*.slopc.json` files. AI agents edit them through MCP or the CLI; people use the web UI. Rendering runs
headless on the GPU (wgpu).

## Build

```sh
cd web && bun install && bun run build && cd ..   # the web UI is embedded in the binary
cargo build --release
```

A GPU with Vulkan, Metal or DX12 is required for rendering (on Linux without a GPU, install lavapipe).

## Use

```sh
slopc new art.slopc.json --width 32 --height 32
slopc nodes                      # node types; `slopc nodes blend` for inputs
slopc edit art.slopc.json ops.json
slopc render art.slopc.json -o art.png --scale 8
slopc serve --root .             # web UI; open the printed URL (it carries the token)
slopc serve --host 0.0.0.0       # reachable from the LAN; put TLS in front for untrusted networks
```

MCP over stdio (for example in an agent's MCP config):

```json
{ "command": "slopc", "args": ["mcp", "--root", "/path/to/projects"] }
```

MCP over HTTP: `http://<host>:7420/mcp` with `Authorization: Bearer <token>` while `slopc serve` runs.

## Develop

```sh
cargo test                       # core, CLI, MCP, server
cd web && bun test src           # UI logic
cd web && bun run e2e            # headless Chromium against target/debug/slopc
```

Specs with numbered test cases live in `spec/`; the plan and feedback in `TODO.md`; agent rules in `AGENTS.md`.
