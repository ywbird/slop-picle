// WEB-29: the export dialog writes a GIF into the root and links to it (spec/export.md).
import { afterAll, beforeAll, expect, test } from "bun:test";
import { chromium, type Browser, type Page } from "playwright-core";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = join(import.meta.dir, "..", "..");
const bin = join(root, "target", "debug", "slopc");
const dir = mkdtempSync(join(tmpdir(), "slopc-export-"));

let server: ReturnType<typeof Bun.spawn>;
let browser: Browser;
let page: Page;

beforeAll(async () => {
  writeFileSync(
    join(dir, "e.slopc.json"),
    JSON.stringify({
      slopc: 1,
      settings: { width: 4, height: 4, frames: 4, fps: 4 },
      nodes: {
        s: { type: "solid", pos: [0, 0], inputs: { color: { anim: [{ time: 0, value: "#000000" }, { time: 3, value: "#ffffff" }] } } },
        out: { type: "output", pos: [260, 0], inputs: { surface_in: { ref: "s" } } },
      },
    }),
  );
  server = Bun.spawn([bin, "serve", "--port", "0", "--root", dir, "--token", "t"], { stdout: "pipe" });
  const reader = server.stdout.getReader();
  let text = "";
  while (!text.includes("\n")) text += new TextDecoder().decode((await reader.read()).value);
  const url = text.trim().replace("slopc serve: ", "");
  browser = await chromium.launch({ executablePath: process.env.CHROMIUM ?? "/usr/sbin/chromium" });
  page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
  await page.goto(url);
  await page.goto(url.split("/?")[0] + "/#/edit/e.slopc.json");
  await page.waitForSelector(".svelte-flow__node");
}, 30000);

afterAll(async () => {
  await browser?.close();
  server?.kill();
});

test("WEB-29 export dialog writes a GIF and links to it", async () => {
  await page.locator("header button", { hasText: "Export" }).click();
  const dialog = page.locator('[role="dialog"][aria-label="Export"]');
  expect(await dialog.locator("input.out").inputValue()).toBe("exports/e.gif");
  await dialog.locator("button.primary").click();
  const link = dialog.locator("a[download]");
  await link.waitFor({ timeout: 10000 });
  expect(await link.textContent()).toBe("exports/e.gif");
  const file = readFileSync(join(dir, "exports", "e.gif"));
  expect(file.subarray(0, 6).toString()).toBe("GIF89a");
  const got = await page.evaluate(async (href) => {
    const res = await fetch(href!);
    const b = new Uint8Array(await res.arrayBuffer());
    return { type: res.headers.get("content-type"), head: String.fromCharCode(...b.slice(0, 6)), size: b.length };
  }, await link.getAttribute("href"));
  expect(got).toEqual({ type: "image/gif", head: "GIF89a", size: file.length });
  await page.keyboard.press("Escape");
  expect(await dialog.count()).toBe(0);
}, 30000);
