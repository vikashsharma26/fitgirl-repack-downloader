<script lang="ts">
  import type { Link } from "./api";

  let {
    links,
    selected = $bindable(),
    queued = new Set<string>(),
  }: {
    links: Link[];
    /** One flag per link. */
    selected: boolean[];
    /** URLs already in the download list. */
    queued?: Set<string>;
  } = $props();

  const optionalCount = $derived(links.filter((l) => l.optional).length);

  function set(pick: (l: Link, i: number) => boolean) {
    selected = links.map(pick);
  }
</script>

<div class="tools">
  <button class="link" onclick={() => set(() => true)}>Select all</button>
  <button class="link" onclick={() => set(() => false)}>Select none</button>
  {#if optionalCount}
    <button class="link" onclick={() => set((l) => !l.optional)}>Main files only</button>
    <span class="muted">{optionalCount} optional (language packs, bonus content)</span>
  {/if}
</div>
<ul>
  {#each links as link, i (link.url)}
    <li>
      <label>
        <input type="checkbox" bind:checked={selected[i]} />
        <span class="name" title={link.filename}
          ><span class="head">{link.filename.slice(0, -14)}</span><span class="tail">{link.filename.slice(-14)}</span></span
        >
        {#if queued.has(link.url)}<span class="tag done">in downloads</span>{/if}
        {#if link.optional}<span class="tag">optional</span>{/if}
      </label>
    </li>
  {/each}
</ul>

<style>
  .tools {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 14px;
    margin-bottom: 8px;
    font-size: 13px;
  }
  .link {
    border: 0;
    padding: 0;
    background: none;
    color: var(--accent);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }
  li + li {
    border-top: 1px solid var(--border);
  }
  label {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 12px;
    cursor: pointer;
  }
  label:hover {
    background: var(--surface-2);
  }
  input {
    accent-color: var(--accent);
    width: 15px;
    height: 15px;
    flex: none;
  }
  .name {
    display: flex;
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    font-family: var(--mono);
    font-size: 12.5px;
  }
  .head {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tail {
    flex: none;
  }
  .tag {
    flex: none;
    padding: 1px 8px;
    border-radius: 99px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 11.5px;
    font-weight: 600;
  }
  .tag.done {
    background: color-mix(in srgb, var(--ok) 14%, transparent);
    color: var(--ok);
  }
</style>
