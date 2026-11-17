// WEB-09, WEB-10: the served UI in headless Chromium. Needs `cargo build` (with web/dist built) and Chromium.
import { afterAll, beforeAll, expect, test } from "bun:test";
import { chromium, type Browser, type Page } from "playwright-core";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = join(import.meta.dir, "..", "..");
const bin = join(root, "target", "debug", "picle");
const chrome = process.env.CHROMIUM ?? "/usr/sbin/chromium";
const dir = mkdtempSync(join(tmpdir(), "picle-e2e-"));
const file = join(dir, "p.picle.json");

let server: ReturnType<typeof Bun.spawn>;
let browser: Browser;
let page: Page;
let url = "";

const read = () => JSON.parse(readFileSync(file, "utf8"));
const nodeCount = () => page.locator(".svelte-flow__node").count();

beforeAll(async () => {
  writeFileSync(
    file,
    JSON.stringify({
      picle: 1,
      settings: { width: 16, height: 16 },
      nodes: {
        bg: { type: "solid", inputs: { color: "#336699" }, pos: [0, 0] },
        n: { type: "noise", pos: [0, 420] },
        out: { type: "output", inputs: { surface_in: { ref: "bg" } }, pos: [320, 0] },
      },
    }),
  );
  server = Bun.spawn([bin, "serve", "--port", "0", "--root", dir, "--token", "e2e"], { stdout: "pipe" });
  const reader = server.stdout.getReader();
  let text = "";
  while (!text.includes("\n")) text += new TextDecoder().decode((await reader.read()).value);
  url = text.trim().replace("picle serve: ", "");
  browser = await chromium.launch({ executablePath: chrome, headless: true });
  page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
  await page.goto(url);
  await page.goto(url.split("/?")[0] + "/#/edit/p.picle.json");
  await page.waitForSelector(".svelte-flow__node");
}, 30000);

afterAll(async () => {
  await page?.screenshot({ path: "/tmp/opencode/ui.png" }).catch(() => {});
  await browser?.close();
  server?.kill();
});

test("WEB-09 nodes, thumbnails and live external edits", async () => {
  expect(await nodeCount()).toBe(3);
  await page.waitForFunction(() => {
    const imgs = [...document.querySelectorAll<HTMLImageElement>(".svelte-flow__node img")];
    return imgs.length === 3 && imgs.every((i) => i.complete && i.naturalWidth === 16);
  });
  const edit = Bun.spawn([bin, "edit", file, "-"], { stdin: "pipe" });
  edit.stdin.write(JSON.stringify([{ op: "add_node", id: "extra", type: "checker", pos: [700, 420] }]));
  edit.stdin.end();
  expect(await edit.exited).toBe(0);
  await page.waitForFunction(() => document.querySelectorAll(".svelte-flow__node").length === 4, null, { timeout: 5000 });
}, 30000);

test("WEB-10 connect and edit a widget", async () => {
  const from = page.locator('.svelte-flow__node[data-id="n"] .svelte-flow__handle[data-handleid="surface_out"]');
  const to = page.locator('.svelte-flow__node[data-id="out"] .svelte-flow__handle[data-handleid="surface_in"]');
  const a = (await from.boundingBox())!;
  const b = (await to.boundingBox())!;
  await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2, { steps: 20 });
  await page.mouse.up();
  for (let i = 0; i < 50 && read().nodes.out.inputs.surface_in.ref !== "n"; i++) await Bun.sleep(100);
  expect(read().nodes.out.inputs.surface_in).toEqual({ ref: "n" });

  await page.locator('.svelte-flow__node[data-id="n"] .head').click();
  const seed = page.locator(".popup .row", { hasText: "seed" }).locator("input");
  await seed.fill("5");
  await seed.press("Enter");
  for (let i = 0; i < 50 && read().nodes.n.inputs?.seed !== 5; i++) await Bun.sleep(100);
  expect(read().nodes.n.inputs.seed).toBe(5);
}, 30000);

test("WEB-11 upload a PNG into an image node", async () => {
  const add = Bun.spawn([bin, "edit", file, "-"], { stdin: "pipe" });
  add.stdin.write(JSON.stringify([{ op: "add_node", id: "pic", type: "image", inputs: { path: "none.png" }, pos: [640, 0] }]));
  add.stdin.end();
  expect(await add.exited).toBe(0);
  await page.waitForSelector('.svelte-flow__node[data-id="pic"]');
  await page.locator('.svelte-flow__node[data-id="pic"] .head').click();
  // A 3x2 PNG made with the picle CLI.
  const png = join(dir, "src.png");
  const mk = join(dir, "mk.picle.json");
  writeFileSync(mk, JSON.stringify({ picle: 1, settings: { width: 3, height: 2 }, nodes: { s: { type: "solid" } } }));
  expect(await Bun.spawn([bin, "render", mk, "-o", png, "--node", "s"]).exited).toBe(0);
  await page.locator('.popup input[type="file"]').setInputFiles(png);
  for (let i = 0; i < 50 && read().nodes.pic.inputs.path !== "assets/src.png"; i++) await Bun.sleep(100);
  expect(read().nodes.pic.inputs.path).toBe("assets/src.png");
  expect(readFileSync(join(dir, "assets", "src.png")).length).toBeGreaterThan(0);
  await page.waitForFunction(() => {
    const img = document.querySelector<HTMLImageElement>('.svelte-flow__node[data-id="pic"] img');
    return !!img && img.complete && img.naturalWidth === 3;
  });
}, 30000);

test("WEB-12 dragging follows the cursor without reloads", async () => {
  const node = page.locator('.svelte-flow__node[data-id="bg"]');
  const pos = () => node.evaluate((e) => (e as HTMLElement).style.transform);
  const start = await pos();
  const reqs: string[] = [];
  const onReq = (r: { url(): string }) => reqs.push(r.url());
  page.on("request", onReq);
  const h = (await node.locator(".head").boundingBox())!;
  await page.mouse.move(h.x + 20, h.y + 10);
  await page.mouse.down();
  const seen: string[] = [];
  for (let i = 1; i <= 30; i++) {
    await page.mouse.move(h.x + 20 + i * 4, h.y + 10 + i * 2);
    await Bun.sleep(20);
    seen.push(await pos());
  }
  const during = reqs.filter((u) => u.includes("/api/project")).length;
  await page.mouse.up();
  await Bun.sleep(500);
  page.off("request", onReq);
  expect(during).toBe(0);
  for (let i = 1; i < seen.length; i++) expect(seen[i]).not.toBe(start);
  expect(new Set(seen).size).toBeGreaterThan(20);
  expect(reqs.filter((u) => u.includes("/api/ops")).length).toBe(1);
}, 30000);


async function menuOn(selector: string, item: string) {
  await page.locator(selector).first().click({ button: "right" });
  await page.locator(".menu button", { hasText: item }).click();
}

test("WEB-20 box select and group move", async () => {
  const box = async (id: string) => (await page.locator(`.svelte-flow__node[data-id="${id}"]`).boundingBox())!;
  const a = await box("bg");
  const b = await box("n");
  // Nodes touching the box are selected; keep the box on screen.
  const x0 = Math.min(a.x, b.x) - 12, y0 = Math.min(a.y, b.y) - 12;
  const x1 = Math.max(a.x + a.width, b.x + b.width) + 12, y1 = Math.min(Math.max(a.y, b.y) + 40, 880);
  await page.mouse.move(x1, y1);
  await page.mouse.down();
  await page.mouse.move(x0, y0, { steps: 15 });
  await page.mouse.up();
  const sel = await page.evaluate(() => [...document.querySelectorAll(".svelte-flow__node.selected")].map((e) => e.getAttribute("data-id")));
  expect(sel).toContain("bg");
  expect(sel).toContain("n");
  const before = read().nodes;
  const bodies: string[] = [];
  const onReq = (r: { url(): string; postData(): string | null }) => r.url().includes("/api/ops") && bodies.push(r.postData() ?? "");
  page.on("request", onReq);
  const h = (await page.locator('.svelte-flow__node[data-id="n"] .head').boundingBox())!;
  await page.mouse.move(h.x + 30, h.y + 8);
  await page.mouse.down();
  await page.mouse.move(h.x + 90, h.y + 48, { steps: 10 });
  await page.mouse.up();
  for (let i = 0; i < 50 && JSON.stringify(read().nodes.bg.pos) === JSON.stringify(before.bg.pos); i++) await Bun.sleep(100);
  page.off("request", onReq);
  const after = read().nodes;
  expect(after.bg.pos).not.toEqual(before.bg.pos);
  expect(after.n.pos).not.toEqual(before.n.pos);
  expect(bodies.length).toBe(1);
  const ops = JSON.parse(bodies[0]);
  expect(ops.filter((o: any) => o.op === "move_node").map((o: any) => o.id)).toEqual(expect.arrayContaining(["bg", "n"]));
  await page.locator(".svelte-flow__pane").click({ position: { x: 5, y: 400 } });
}, 30000);

test("WEB-19 line grid background", async () => {
  expect(await page.locator(".svelte-flow__background").count()).toBe(2);
  expect(await page.locator(".svelte-flow__background pattern path").count()).toBeGreaterThanOrEqual(2);
});

test("WEB-17 no attribution link", async () => {
  expect(await page.locator(".svelte-flow__attribution").count()).toBe(0);
});

test("WEB-15 click a node opens its popup beside it; Esc closes", async () => {
  await page.locator('.svelte-flow__node[data-id="n"] .head').click();
  const pop = page.locator(".popup");
  await pop.waitFor();
  expect(await pop.locator(".row", { hasText: "seed" }).count()).toBe(1);
  const nb = (await page.locator('.svelte-flow__node[data-id="n"]').boundingBox())!;
  const pb = (await pop.boundingBox())!;
  expect(Math.abs(pb.y - nb.y)).toBeLessThan(200);
  await page.keyboard.press("Escape");
  await pop.waitFor({ state: "detached" });
}, 30000);

test("WEB-16 settings panel: collapsed by default, only its toggle opens it", async () => {
  const collapsed = () => page.evaluate(() => document.querySelector("aside.panel")!.classList.contains("collapsed"));
  expect(await collapsed()).toBe(true);
  await page.locator('.svelte-flow__node[data-id="bg"] .head').click();
  await Bun.sleep(300);
  expect(await collapsed()).toBe(true);
  await page.keyboard.press("Escape");
  await page.locator("aside.panel .toggle").click();
  await page.waitForFunction(() => !document.querySelector("aside.panel")!.classList.contains("collapsed"));
  expect(await page.locator("aside.panel input[type=number]").count()).toBe(4);
  expect(await page.locator("aside.panel img").count()).toBe(0);
  await page.locator("aside.panel .toggle").click();
  await page.waitForFunction(() => document.querySelector("aside.panel")!.getBoundingClientRect().width < 60);
}, 30000);

test("WEB-18 thumbnail opens the image popup", async () => {
  await page.locator('.svelte-flow__node[data-id="bg"] img.thumb').click();
  const card = page.locator(".image-popup");
  await card.waitFor();
  const img = card.locator("img");
  await page.waitForFunction(() => {
    const i = document.querySelector<HTMLImageElement>(".image-popup img");
    return !!i && i.complete && i.naturalWidth === 16;
  });
  const zoom = card.locator('input[type="range"]');
  await zoom.fill("4");
  const w4 = (await img.boundingBox())!.width;
  await zoom.fill("12");
  const w12 = (await img.boundingBox())!.width;
  expect(Math.round(w12 / w4)).toBe(3);
  expect(await card.locator('a[download]').getAttribute("href")).toContain("/api/render");
  await page.keyboard.press("Escape");
  await card.waitFor({ state: "detached" });
}, 30000);

test("WEB-14 context menus", async () => {
  const waitFile = async (ok: (p: any) => boolean) => {
    for (let i = 0; i < 50 && !ok(read()); i++) await Bun.sleep(100);
    expect(ok(read())).toBe(true);
  };
  // Duplicate n (a noise with seed 5).
  await menuOn('.svelte-flow__node[data-id="n"] .head', "Duplicate");
  await waitFile((p) => Object.values(p.nodes).filter((x: any) => x.type === "noise" && x.inputs?.seed === 5).length === 2);
  // Delete the checker added in WEB-09.
  await menuOn('.svelte-flow__node[data-id="extra"] .head', "Delete");
  await waitFile((p) => !p.nodes.extra);
  // Disconnect out.surface_in.
  const edge = page.locator('.svelte-flow__edge[data-id="n.surface_out->out.surface_in"] .svelte-flow__edge-interaction');
  await edge.dispatchEvent("contextmenu", { button: 2 });
  await page.locator(".menu button", { hasText: "Disconnect" }).click();
  await waitFile((p) => !p.nodes.out.inputs?.surface_in);
  // Canvas menu: add a solid at the cursor.
  const before = Object.keys(read().nodes).length;
  await page.locator(".svelte-flow__pane").click({ button: "right", position: { x: 60, y: 820 } });
  await page.locator(".menu input").fill("solid");
  await page.keyboard.press("Enter");
  await waitFile((p) => Object.keys(p.nodes).length === before + 1);
}, 30000);

async function waitFor(ok: (p: any) => boolean) {
  for (let i = 0; i < 50 && !ok(read()); i++) await Bun.sleep(100);
  expect(ok(read())).toBe(true);
}

async function boxSelect(ids: string[]) {
  const boxes = await Promise.all(ids.map(async (id) => (await page.locator(`.svelte-flow__node[data-id="${id}"]`).boundingBox())!));
  const x0 = Math.min(...boxes.map((b) => b.x)) - 12, y0 = Math.min(...boxes.map((b) => b.y)) - 12;
  const x1 = Math.max(...boxes.map((b) => b.x + b.width)) + 12, y1 = Math.min(Math.max(...boxes.map((b) => b.y)) + 40, 880);
  await page.mouse.move(x1, y1);
  await page.mouse.down();
  await page.mouse.move(x0, y0, { steps: 15 });
  await page.mouse.up();
}

test("WEB-21 Delete key removes the selected node", async () => {
  await page.locator('.svelte-flow__node[data-id="pic"] .head').click();
  await page.keyboard.press("Delete");
  await waitFor((p) => !p.nodes.pic);
}, 30000);

test("WEB-22 Delete removes a box selection in one request", async () => {
  await page.keyboard.press("Escape");
  await page.locator(".svelte-flow__pane").click({ position: { x: 5, y: 400 } });
  const bodies: string[] = [];
  const onReq = (r: { url(): string; postData(): string | null }) => r.url().includes("/api/ops") && bodies.push(r.postData() ?? "");
  page.on("request", onReq);
  await boxSelect(["bg", "n"]);
  await page.keyboard.press("Delete");
  await waitFor((p) => !p.nodes.bg && !p.nodes.n);
  page.off("request", onReq);
  expect(bodies.length).toBe(1);
}, 30000);

test("WEB-23 undo and redo by shortcut and button", async () => {
  await page.keyboard.press("Control+z");
  await waitFor((p) => !!p.nodes.bg && !!p.nodes.n);
  await page.keyboard.press("Control+Shift+z");
  await waitFor((p) => !p.nodes.bg && !p.nodes.n);
  await page.locator('header button[aria-label="undo"]').click();
  await waitFor((p) => !!p.nodes.bg && !!p.nodes.n);
}, 30000);

test("WEB-24 undo keeps an edit made elsewhere", async () => {
  await page.waitForSelector('.svelte-flow__node[data-id="n"]');
  await page.locator('.svelte-flow__node[data-id="n"] .head').click();
  const seed = page.locator(".popup .row", { hasText: "seed" }).locator("input");
  await seed.fill("9");
  await seed.press("Enter");
  await waitFor((p) => p.nodes.n.inputs?.seed === 9);
  const edit = Bun.spawn([bin, "edit", file, "-"], { stdin: "pipe" });
  edit.stdin.write(JSON.stringify([{ op: "add_node", id: "ext", type: "solid", pos: [0, -200] }]));
  edit.stdin.end();
  expect(await edit.exited).toBe(0);
  await page.keyboard.press("Escape");
  await page.locator(".svelte-flow__pane").click({ position: { x: 5, y: 400 } });
  await page.keyboard.press("Control+z");
  await waitFor((p) => p.nodes.n.inputs?.seed === 5);
  expect(read().nodes.ext).toBeTruthy();
}, 30000);
