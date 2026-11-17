<script lang="ts">
  let {
    frame,
    frames,
    fps,
    setFrame,
  }: { frame: number; frames: number; fps: number; setFrame: (f: number) => Promise<unknown> } = $props();

  let playing = $state(false);

  // Show each next frame once it is ready, at most `fps` per second (spec/animation.md, Web UI).
  async function play() {
    while (playing) {
      const t0 = performance.now();
      await setFrame((frame + 1) % frames);
      const rest = 1000 / Math.max(1, fps) - (performance.now() - t0);
      if (rest > 0) await new Promise((r) => setTimeout(r, rest));
    }
  }

  export function toggle() {
    playing = !playing;
    if (playing) play();
  }
  const step = (d: number) => setFrame((frame + d + frames) % frames);
</script>

<div class="timeline">
  <button aria-label="first frame" title="First frame" onclick={() => setFrame(0)}>⏮</button>
  <button aria-label="previous frame" title="Previous frame (,)" onclick={() => step(-1)}>‹</button>
  {#if playing}
    <button class="primary" aria-label="pause" title="Pause (Space)" onclick={toggle}>❚❚</button>
  {:else}
    <button aria-label="play" title="Play (Space)" onclick={toggle}>▶</button>
  {/if}
  <button aria-label="next frame" title="Next frame (.)" onclick={() => step(1)}>›</button>
  <input type="range" min="0" max={frames - 1} value={frame} aria-label="frame" oninput={(e) => setFrame(Number(e.currentTarget.value))} />
  <span class="frame">{frame}</span>
  <span class="muted">/ {frames - 1} · {fps} fps</span>
</div>

<style>
  .timeline { display: flex; gap: 6px; align-items: center; padding: 6px 12px; border-top: 1px solid var(--line); background: var(--surface); }
  button { width: 32px; padding: 0; }
  input[type="range"] { flex: 1; }
  .frame { font-family: var(--font-mono); min-width: 3ch; text-align: right; }
</style>
