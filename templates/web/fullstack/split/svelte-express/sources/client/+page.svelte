<script>
  import { projectLinks } from "../config/framework";
  let links = projectLinks;
</script>

<svelte:head>
  <title>{{project_name}}</title>
</svelte:head>

<main>
  <h1>Welcome to {{project_name}}</h1>
  <p>MegaGate SvelteKit scaffold ready.</p>
  <ul>
    {#each links as link}
      <li><a href={link.url}>{link.label}</a></li>
    {/each}
  </ul>
</main>

<style>
  main {
    max-width: 600px;
    margin: 4rem auto;
    padding: 0 1rem;
    font-family: system-ui, sans-serif;
  }
  h1 { font-size: 2rem; margin-bottom: 0.5rem; }
  ul { list-style: none; padding: 0; }
  li { margin: 0.5rem 0; }
  a { color: #646cff; text-decoration: none; }
  a:hover { text-decoration: underline; }
</style>
