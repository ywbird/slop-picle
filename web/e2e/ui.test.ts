// WEB-09, WEB-10: the served UI in headless Chromium. Needs `cargo build` (with web/dist built) and Chromium.
import { afterAll, beforeAll, expect, test } from "bun:test";
import { chromium, type Browser, type Page } from "playwright-core";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = join(import.meta.dir, "..", "..");
const bin = join(root, "target", "debug", "slopc");
const chrome = process.env.CHROMIUM ?? "/usr/sbin/chromium";
const dir = mkdtempSync(join(tmpdir(), "slopc-e2e-"));
const file = join(dir, "p.slopc.json");

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
      slopc: 1,
      settings: { width: 16, height: 16 },
      nodes: {
        bg: { type: "solid", inputs: { color: "#336699" }, pos: [0, 0] },
        n: { type: "noise", pos: [0, 260] },
        out: { type: "output", inputs: { surface_in: { ref: "bg" } }, pos: [320, 0] },
      },
    }),
  );
  server = Bun.spawn([bin, "serve", "--port", "0", "--root", dir, "--token", "e2e"], { stdout: "pipe" });
  const reader = server.stdout.getReader();
  let text = "";
  while (!text.includes("\n")) text += new TextDecoder().decode((await reader.read()).value);
  url = text.trim().replace("slopc serve: ", "");
  browser = await chromium.launch({ executablePath: chrome, headless: true });
  page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
  await page.goto(url);
  await page.goto(url.split("/?")[0] + "/#/edit/p.slopc.json");
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
  edit.stdin.write(JSON.stringify([{ op: "add_node", id: "extra", type: "checker", pos: [320, 260] }]));
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
  const seed = page.locator("aside .row", { hasText: "seed" }).locator("input");
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
  // A 3x2 PNG made with the slopc CLI.
  const png = join(dir, "src.png");
  const mk = join(dir, "mk.slopc.json");
  writeFileSync(mk, JSON.stringify({ slopc: 1, settings: { width: 3, height: 2 }, nodes: { s: { type: "solid" } } }));
  expect(await Bun.spawn([bin, "render", mk, "-o", png, "--node", "s"]).exited).toBe(0);
  await page.locator('aside input[type="file"]').setInputFiles(png);
  for (let i = 0; i < 50 && read().nodes.pic.inputs.path !== "assets/src.png"; i++) await Bun.sleep(100);
  expect(read().nodes.pic.inputs.path).toBe("assets/src.png");
  expect(readFileSync(join(dir, "assets", "src.png")).length).toBeGreaterThan(0);
  await page.waitForFunction(() => {
    const img = document.querySelector<HTMLImageElement>('.svelte-flow__node[data-id="pic"] img');
    return !!img && img.complete && img.naturalWidth === 3;
  });
}, 30000);
