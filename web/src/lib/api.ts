// Fetch wrapper for the picle API (cookie auth; errors keep the server's shape).
import type { ApiError, Catalog, Op, ProjectFile } from "./graph";

export class ApiFailure extends Error {
  constructor(public status: number, public errors: ApiError[]) {
    super(errors.map((e) => e.message).join("; ") || `HTTP ${status}`);
  }
}

async function call<T>(method: string, url: string, body?: unknown): Promise<T> {
  const res = await fetch(url, {
    method,
    headers: body === undefined ? {} : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
    credentials: "same-origin",
  });
  if (res.status === 204) return undefined as T;
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new ApiFailure(res.status, data.errors ?? []);
  return data as T;
}

export type ProjectView = {
  project: ProjectFile;
  errors: ApiError[];
  keys: Record<string, string>;
  animated: Record<string, Record<string, unknown>>;
};
export type OpResult = { ok: true; created: string[]; warnings: ApiError[]; undo: Op[]; redo: Op[] };

const q = (path: string) => `path=${encodeURIComponent(path)}`;

export const api = {
  login: (token: string) => call<void>("POST", "/api/login", { token }),
  catalog: () => call<Catalog>("GET", "/api/catalog"),
  projects: () => call<{ projects: string[] }>("GET", "/api/projects"),
  create: (path: string, width: number, height: number) => call<unknown>("POST", "/api/projects", { path, width, height }),
  project: (path: string, frame = 0) => call<ProjectView>("GET", `/api/project?${q(path)}&frame=${frame}`),
  ops: (path: string, ops: Op[]) => call<OpResult>("POST", `/api/ops?${q(path)}`, ops),
  upload: async (path: string, file: Blob) => {
    const res = await fetch(`/api/upload?${q(path)}`, { method: "POST", body: file, credentials: "same-origin" });
    const data = await res.json().catch(() => ({}));
    if (!res.ok) throw new ApiFailure(res.status, data.errors ?? []);
  },
  export: (path: string, options: Record<string, unknown>) =>
    call<{ ok: true; files: string[]; frames: number[] }>("POST", `/api/export?${q(path)}`, options),
  fileUrl: (path: string) => `/api/file?${q(path)}`,
  renderUrl: (path: string, node: string | null, frame: number, key = "") =>
    `/api/render?${q(path)}${node ? `&node=${encodeURIComponent(node)}` : ""}&frame=${frame}&k=${encodeURIComponent(key)}`,
  events: (path: string, onChange: (keys: Record<string, string>) => void): (() => void) => {
    const es = new EventSource(`/api/events?${q(path)}`);
    es.addEventListener("changed", (e) => onChange(JSON.parse((e as MessageEvent).data).keys));
    return () => es.close();
  },
};
