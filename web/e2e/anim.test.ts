// WEB-25, WEB-26: timeline and keyframes in headless Chromium (spec/animation.md).
import { afterAll, beforeAll, expect, test } from "bun:test";
import { chromium, type Browser, type Page } from "playwright-core";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = join(import.meta.dir, "..", "..");
const bin = join(root, "target", "debug", "picle");
const dir = mkdtempSync(join(tmpdir(), "picle-anim-"));
const file = join(dir, "a.picle.json");
const read = () => JSON.parse(readFileSync(file, "utf8"));
const grey = (i: number) => "#" + (i * 60).toString(16).padStart(2, "0").repeat(3);

let server: ReturnType<typeof Bun.spawn>;
let browser: Browser;
let page: Page;

beforeAll(async () => {
  writeFileSync(
    file,
    JSON.stringify({
      picle: 1,
      settings: { width: 4, height: 4, frames: 4, fps: 4 },
      nodes: {
        a: { type: "solid", pos: [0, 0], inputs: { color: { anim: [0, 1, 2, 3].map((i) => ({ time: i, value: grey(i), ease_out_type: "cut" })) } } },
        k: { type: "solid", pos: [0, 240] },
      },
    }),
  );
  server = Bun.spawn([bin, "serve", "--port", "0", "--root", dir, "--token", "t"], { stdout: "pipe" });
  const reader = server.stdout.getReader();
  let text = "";
  while (!text.includes("\n")) text += new TextDecoder().decode((await reader.read()).value);
  const url = text.trim().replace("picle serve: ", "");
  browser = await chromium.launch({ executablePath: process.env.CHROMIUM ?? "/usr/sbin/chromium" });
  page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
  await page.goto(url);
  await page.goto(url.split("/?")[0] + "/#/edit/a.picle.json");
  await page.waitForSelector(".svelte-flow__node");
}, 30000);

afterAll(async () => {
  await browser?.close();
  server?.kill();
});

/** Red channel of node `id`'s thumbnail. */
async function thumbRed(id: string): Promise<number> {
  await page.waitForFunction((id) => {
    const i = document.querySelector<HTMLImageElement>(`.svelte-flow__node[data-id="${id}"] img.thumb`);
    return !!i && i.complete && i.naturalWidth > 0;
  }, id);
  return page.evaluate((id) => {
    const img = document.querySelector<HTMLImageElement>(`.svelte-flow__node[data-id="${id}"] img.thumb`)!;
    const c = document.createElement("canvas");
    c.width = img.naturalWidth;
    c.height = img.naturalHeight;
    const g = c.getContext("2d")!;
    g.drawImage(img, 0, 0);
    return g.getImageData(0, 0, 1, 1).data[0];
  }, id);
}

const frameText = () => page.locator(".timeline .frame").innerText();

test("WEB-25 timeline steps and plays", async () => {
  await page.waitForSelector(".timeline");
  expect(await frameText()).toBe("0");
  expect(await thumbRed("a")).toBe(0);
  await page.locator('.timeline button[aria-label="next frame"]').click();
  await page.waitForFunction(() => document.querySelector(".timeline .frame")?.textContent === "1");
  await page.waitForFunction(() => document.querySelector<HTMLImageElement>('.svelte-flow__node[data-id="a"] img.thumb')?.src.includes("frame=1"));
  for (let i = 0; i < 30 && (await thumbRed("a")) !== 60; i++) await Bun.sleep(100);
  expect(await thumbRed("a")).toBe(60);
  await page.locator('.timeline button[aria-label="play"]').click();
  // 4 frames at 4 fps loop every second, so check the frames visited rather than where it stops.
  const visited = new Set<string>();
  for (let t0 = Date.now(); Date.now() - t0 < 1200; await Bun.sleep(20)) visited.add(await frameText());
  await page.locator('.timeline button[aria-label="pause"]').click();
  expect(visited.size).toBe(4);
}, 30000);

test("WEB-26 key buttons set and show keys", async () => {
  await page.locator('.timeline button[aria-label="first frame"]').click();
  await page.waitForFunction(() => document.querySelector(".timeline .frame")?.textContent === "0");
  await page.locator('.svelte-flow__node[data-id="k"] .head').click();
  const row = page.locator(".popup .row", { hasText: "color" });
  await row.locator("button.key").click();
  for (let i = 0; i < 50 && !read().nodes.k.inputs?.color?.anim; i++) await Bun.sleep(100);
  expect(read().nodes.k.inputs.color.anim.map((k: any) => k.time)).toEqual([0]);
  await page.locator('.timeline button[aria-label="next frame"]').click();
  await page.locator('.timeline button[aria-label="next frame"]').click();
  await page.waitForFunction(() => document.querySelector(".timeline .frame")?.textContent === "2");
  await row.locator('input[type="color"]').evaluate((el: HTMLInputElement) => {
    el.value = "#00ff00";
    el.dispatchEvent(new Event("change", { bubbles: true }));
  });
  for (let i = 0; i < 50 && read().nodes.k.inputs.color.anim.length < 2; i++) await Bun.sleep(100);
  expect(read().nodes.k.inputs.color.anim.map((k: any) => k.time)).toEqual([0, 2]);
  await page.waitForFunction(() => [...document.querySelectorAll(".popup .row")].find((r) => r.textContent?.includes("color"))?.querySelector("button.key")?.classList.contains("on"));
  await page.locator('.timeline button[aria-label="previous frame"]').click();
  await page.waitForFunction(() => document.querySelector(".timeline .frame")?.textContent === "1");
  await page.waitForFunction(() => ![...document.querySelectorAll(".popup .row")].find((r) => r.textContent?.includes("color"))?.querySelector("button.key")?.classList.contains("on"));
}, 30000);

test("WEB-28 stepping frames neither reopens events nor reloads twice", async () => {
  const urls: string[] = [];
  const onReq = (r: { url(): string }) => urls.push(r.url());
  page.on("request", onReq);
  for (let i = 0; i < 5; i++) {
    const want = String((Number(await frameText()) + 1) % 4);
    await page.locator('.timeline button[aria-label="next frame"]').click();
    await page.waitForFunction((w) => document.querySelector(".timeline .frame")?.textContent === w, want);
  }
  await Bun.sleep(300);
  page.off("request", onReq);
  expect(urls.filter((u) => u.includes("/api/events")).length).toBe(0);
  expect(urls.filter((u) => u.includes("/api/project")).length).toBe(5);
}, 30000);
