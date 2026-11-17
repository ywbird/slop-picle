<script lang="ts">
  import { api, ApiFailure } from "./lib/api";
  import type { Catalog } from "./lib/graph";
  import Projects from "./Projects.svelte";
  import Editor from "./Editor.svelte";

  let hash = $state(location.hash);
  let catalog: Catalog | null = $state(null);
  let needLogin = $state(false);
  let token = $state("");
  let loginError = $state("");

  window.addEventListener("hashchange", () => (hash = location.hash));
  const editing = $derived(hash.startsWith("#/edit/") ? decodeURIComponent(hash.slice(7)) : null);

  async function load() {
    try {
      catalog = await api.catalog();
      needLogin = false;
    } catch (e) {
      if (e instanceof ApiFailure && e.status === 401) needLogin = true;
      else throw e;
    }
  }

  async function login(ev: SubmitEvent) {
    ev.preventDefault();
    try {
      await api.login(token.trim());
      loginError = "";
      await load();
    } catch {
      loginError = "Wrong token.";
    }
  }

  load();
</script>

{#if needLogin}
  <form class="login" onsubmit={login}>
    <h1>slopc</h1>
    <p class="muted">Paste the token printed by <code>slopc serve</code>.</p>
    <input type="password" placeholder="token" bind:value={token} aria-label="token" />
    <button type="submit">Open</button>
    {#if loginError}<p class="error">{loginError}</p>{/if}
  </form>
{:else if catalog}
  {#if editing}
    {#key editing}
      <Editor path={editing} {catalog} />
    {/key}
  {:else}
    <Projects />
  {/if}
{/if}

<style>
  .login { max-width: 320px; margin: 15vh auto; display: flex; flex-direction: column; gap: 8px; }
  h1 { margin: 0; color: var(--accent); }
</style>
