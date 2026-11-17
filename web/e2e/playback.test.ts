// WEB-27: playback keeps the timeline and the pictures in step (spec/web.md, spec/animation.md).
import { afterAll, beforeAll, expect, test } from "bun:test";
import { chromium, type Browser, type Page } from "playwright-core";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = join(import.meta.dir, "..", "..");
const bin = join(root, "target", "debug", "slopc");
const dir = mkdtempSync(join(tmpdir(), "slopc-play-"));
const FRAMES = 24;

let server: ReturnType<typeof Bun.spawn>;
let browser: Browser;
let page: Page;

beforeAll(async () => {
  writeFileSync(
    join(dir, "p.slopc.json"),
    JSON.stringify({
      slopc: 1,
      settings: { width: 32, height: 32, frames: FRAMES, fps: 12 },
      nodes: {
        g: { type: "gradient", pos: [0, 0], inputs: { type: "radial", angle: { anim: [{ time: 0, value: 0 }, { time: FRAMES, value: 360 }] },
          gradient: { type: "constant", keys: [{ time: 0, color: "#f4e4c1" }, { time: 0.25, color: "#a8743c" }, { time: 0.5, color: "#e0b070" }, { time: 0.75, color: "#a8743c" }] } } },
        d: { type: "dither", pos: [260, 0], inputs: { surface_in: { ref: "g" }, palette: ["#1a1c2c", "#a8743c", "#f4e4c1"] } },
        t: { type: "transform", pos: [520, 0], inputs: { surface_in: { ref: "d" }, rotation: 12, scale: [1, 0.5] } },
        o: { type: "outline", pos: [780, 0], inputs: { surface_in: { ref: "t" }, width: 1, profile: "square", color: "#1a1c2c" } },
        bg: { type: "solid", pos: [780, 260], inputs: { color: "#0b0d17" } },
        b: { type: "blend", pos: [1040, 0], inputs: { background: { ref: "bg" }, foreground: { ref: "o" } } },
        out: { type: "output", pos: [1300, 0], inputs: { surface_in: { ref: "b" } } },
      },
    }),
  );
  server = Bun.spawn([bin, "serve", "--port", "0", "--root", dir, "--token", "t"], { stdout: "pipe" });
  const reader = server.stdout.getReader();
  let text = "";
  while (!text.includes("\n")) text += new TextDecoder().decode((await reader.read()).value);
  const url = text.trim().replace("slopc serve: ", "");
  browser = await chromium.launch({ executablePath: process.env.CHROMIUM ?? "/usr/sbin/chromium" });
  page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  await page.goto(url);
  await page.goto(url.split("/?")[0] + "/#/edit/p.slopc.json");
  await page.waitForSelector(".timeline");
}, 30000);

afterAll(async () => {
  await browser?.close();
  server?.kill();
});

test("WEB-27 playback shows the frame the timeline names, without skips, and loops back to the same picture", async () => {
  const keys: string[] = await page.evaluate(async (n) => {
    const out: string[] = [];
    for (let f = 0; f < n; f++) out.push((await (await fetch(`/api/project?path=p.slopc.json&frame=${f}`)).json()).keys.out);
    return out;
  }, FRAMES);
  expect(new Set(keys).size).toBe(FRAMES);
  await page.waitForFunction(() => {
    const i = document.querySelector<HTMLImageElement>('.svelte-flow__node[data-id="out"] img.thumb');
    return !!i && i.complete && i.naturalWidth === 32;
  });
  const r = await page.evaluate(async (keys) => {
    const img = () => document.querySelector<HTMLImageElement>('.svelte-flow__node[data-id="out"] img.thumb')!;
    const pixels = () => {
      const i = img();
      const c = document.createElement("canvas");
      c.width = i.naturalWidth;
      c.height = i.naturalHeight;
      const g = c.getContext("2d")!;
      g.drawImage(i, 0, 0);
      return Array.from(g.getImageData(0, 0, c.width, c.height).data).join(",");
    };
    const start = pixels();
    document.querySelector<HTMLButtonElement>('.timeline button[aria-label="play"]')!.click();
    const seq: number[] = [];
    const mismatches: string[] = [];
    let samples = 0;
    let wrapped: string | null = null;
    let seen = 0;
    const t0 = performance.now();
    while (performance.now() - t0 < 40000) {
      const f = Number(document.querySelector(".timeline .frame")!.textContent);
      const cs = img().currentSrc;
      const k = cs ? new URL(cs).searchParams.get("k") : "(none)";
      samples++;
      if (k !== keys[f]) mismatches.push(`frame ${f}: shows ${k}`);
      if (seq[seq.length - 1] !== f) seq.push(f);
      seen = Math.max(seen, f);
      if (seen === keys.length - 1 && f === 0 && wrapped === null) wrapped = pixels();
      if (wrapped !== null && f === 3) break;
      await new Promise((res) => setTimeout(res, 10));
    }
    document.querySelector<HTMLButtonElement>('.timeline button[aria-label="pause"]')?.click();
    return { start, wrapped, seq, mismatches, samples, ms: Math.round(performance.now() - t0) };
  }, keys);
  console.log(`WEB-27: ${r.samples} samples over ${r.ms} ms, ${r.seq.length} frame changes`);
  expect(r.mismatches.slice(0, 5)).toEqual([]);
  const steps = r.seq.slice(1).map((f, i) => (f - r.seq[i] + FRAMES) % FRAMES);
  expect(steps.every((s) => s === 1)).toBe(true);
  expect(r.wrapped).not.toBeNull();
  expect(r.wrapped).toBe(r.start);
}, 60000);
