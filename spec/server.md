# Server

`slopc serve [--root DIR] [--host HOST] [--port PORT] [--token TOKEN]` serves the web UI, a JSON API and (M5) MCP over
Streamable HTTP. The project files on disk stay the source of truth: the server reads them, applies ops under the
project lock, and watches them for changes made by anyone (CLI, MCP, an editor).

## Start

- `--root` defaults to `.`, `--host` to `127.0.0.1`, `--port` to `7420` (`0` picks a free port).
  Reaching it from the LAN needs `--host 0.0.0.0`.
- The token is `--token`, else the `SLOPC_TOKEN` environment variable, else 32 random hex characters.
- The first stdout line is `slopc serve: http://<host>:<port>/?token=<token>` (with the real port), so people can open
  it and tests can parse it.
- Every path argument goes through the root sandbox of [mcp.md](mcp.md#root-sandbox); rendering cannot read files
  outside the root either.

## Auth and request checks

- Everything under `/api/` and `/mcp` needs the token: `Authorization: Bearer <token>`, or the cookie
  `slopc_token=<token>`. Missing or wrong → `401`. Tokens are compared in constant time.
- `GET /?token=<token>` with the right token sets the cookie (`HttpOnly; SameSite=Strict; Path=/`) and redirects
  (`303`) to `/`. A wrong token is ignored (the page loads and asks for a token).
- `POST /api/login` with `{"token": ...}` sets the cookie on success (`204`), else `401`.
- Static files (the web UI) need no token; they contain no project data.
- Requests other than `GET`/`HEAD` that carry an `Origin` header must have an origin whose `host:port` equals the
  `Host` header; otherwise `403`. (Cross-site requests are refused even with a cookie.)
- Request bodies over 16 MiB → `413`.
- Plain HTTP only; TLS belongs to a reverse proxy or tunnel.

## API

Errors are `{"ok": false, "errors": [...]}` (shapes from [project-file.md](project-file.md)) with these statuses:
`400` bad path or malformed request (`bad_path`, `path_outside_root`, `bad_request`), `401`, `403`, `404`
(`not_found`), `409` (`exists`), `413`, `422` (invalid project, rejected ops, evaluation errors), `503`
(`gpu_unavailable`).

| Method and path | Body / query | Result |
|---|---|---|
| `GET /api/catalog` | | catalog array (as `slopc nodes --json`) |
| `GET /api/projects` | | `{ "projects": [...] }` (as MCP `list_projects`) |
| `POST /api/projects` | `{ path, width?, height? }` | `201 { ok, path }` |
| `GET /api/project` | `?path=` | `{ project, errors, keys }`: file JSON, validation errors, cache key per node (hex) |
| `POST /api/ops` | `?path=`, body: ops array | `200` batch result ([ops.md](ops.md)), or `422` with the errors |
| `GET /api/render` | `?path=&node=&frame=&scale=` | `image/png` (scale 1–32, default 1; node default as CLI) |
| `GET /api/events` | `?path=` | Server-Sent Events, see below |
| `POST /api/upload` | `?path=` (a `.png` path), body: PNG bytes | M5: `201 { ok, path }`; not a decodable PNG within the size limits → `400 bad_request` |

`keys` are the evaluator cache keys ([evaluation.md](evaluation.md#cache)): they change exactly when a node's output
can change, so the UI reloads only those thumbnails. Nodes that cannot be keyed (missing inputs, invalid project)
are absent.

### Events

`GET /api/events?path=` streams `text/event-stream`:

- On connect: `event: changed` with `data: {"keys": {...}}` for the current file.
- Whenever the file changes on disk (any writer), within 1 s: another `changed` event with the new keys.
  Several changes close together may be merged into one event.
- A comment line `: ping` every 15 s keeps proxies from closing the stream.

## MCP over HTTP (M5)

`/mcp` serves the tools of [mcp.md](mcp.md) over Streamable HTTP, behind the same token check. rmcp's loopback-only
`Host` allow-list is turned off so LAN and remote clients can connect; the token is what keeps other sites out
(a DNS-rebinding page cannot know it, and the cookie is not sent to another host).

## Static files

The web UI build (`web/dist`) is embedded in the binary. Unknown non-API paths serve `index.html`. A binary built
without the web UI serves a short HTML page saying so.

## Test cases

Tests start `slopc serve --port 0 --root <tempdir> --token test-token` and parse the first line.

| ID | Given | Expected |
|---|---|---|
| SRV-01 | first stdout line | `slopc serve: http://127.0.0.1:<port>/?token=test-token` |
| SRV-02 | `GET /api/catalog` without token / with wrong Bearer / with Bearer | 401 / 401 / 200, array of all catalog types |
| SRV-03 | `GET /?token=test-token` (no redirect following) | 303 to `/`, `Set-Cookie: slopc_token=test-token` with `HttpOnly`, `SameSite=Strict`; that cookie then authorises `/api/catalog` |
| SRV-04 | `POST /api/login` right / wrong token | 204 with cookie / 401 |
| SRV-05 | `POST /api/projects` with `Origin: http://evil.example` / matching origin / no origin | 403 / 201 / 201 (different paths) |
| SRV-06 | body of 17 MiB to `/api/ops` | 413 |
| SRV-07 | create, list, read, apply ops (solid + output), render | 201; listed; `keys` has both nodes; ops 200; PNG equals `slopc render` of the same file |
| SRV-08 | rejected ops | 422 with errors; file unchanged |
| SRV-09 | `path=../x.slopc.json` | 400 `path_outside_root` |
| SRV-10 | read / render a missing project | 404 `not_found` |
| SRV-11 | render a project with no output node and no `node` | 422 `no_target` |
| SRV-12 | subscribe to events, then `slopc edit` the file from another process | first event at connect; a second `changed` event within 2 s; key of the edited node changed, key of an untouched upstream node unchanged |
| SRV-13 | `GET /` and `GET /some/page` | 200 `text/html` |
| SRV-14 | no `--token`, `SLOPC_TOKEN=env-token` | URL shows `env-token`; it authorises |
| SRV-15 | no token anywhere | URL shows a 32-hex-character token |
| SRV-16 | `POST /mcp` `initialize` without token / with Bearer | 401 / a JSON-RPC result |
| SRV-17 | over `/mcp` with a `Host` header that is not loopback (e.g. `192.168.1.5:7420`): initialize, `tools/list`, create, edit, render | six tools; render returns an image |
| SRV-18 | `POST /api/upload?path=assets/a.png` with a PNG; then an `image` node reading it is rendered | 201; the file exists; the render shows its pixels |
| SRV-19 | upload of non-PNG bytes / to `a.txt` / to `../a.png` | 400 `bad_request` / 400 `bad_path` / 400 `path_outside_root` |
