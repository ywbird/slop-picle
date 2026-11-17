<script lang="ts">
  import { api, ApiFailure } from "./lib/api";

  let projects: string[] = $state([]);
  let path = $state("untitled.slopc.json");
  let width = $state(32);
  let height = $state(32);
  let error = $state("");

  async function refresh() {
    projects = (await api.projects()).projects;
  }

  async function create(ev: SubmitEvent) {
    ev.preventDefault();
    try {
      await api.create(path, width, height);
      location.hash = `#/edit/${encodeURIComponent(path)}`;
    } catch (e) {
      error = e instanceof ApiFailure ? e.message : String(e);
    }
  }

  refresh();
</script>

<main>
  <h1>slopc</h1>
  <h2>Projects</h2>
  {#if projects.length === 0}<p class="muted">No projects under the root yet.</p>{/if}
  <ul>
    {#each projects as p (p)}
      <li><a href={`#/edit/${encodeURIComponent(p)}`}>{p}</a></li>
    {/each}
  </ul>
  <h2>New project</h2>
  <form onsubmit={create}>
    <label>Path <input bind:value={path} size="30" /></label>
    <label>Width <input type="number" min="1" max="4096" bind:value={width} /></label>
    <label>Height <input type="number" min="1" max="4096" bind:value={height} /></label>
    <button type="submit">Create</button>
  </form>
  {#if error}<p class="error">{error}</p>{/if}
</main>

<style>
  main { max-width: 720px; margin: 40px auto; padding: 0 16px; }
  h1 { color: var(--accent); }
  a { color: var(--text); }
  form { display: flex; gap: 12px; flex-wrap: wrap; align-items: center; }
</style>
