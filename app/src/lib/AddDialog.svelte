<script lang="ts">
  import { untrack } from "svelte";
  import Modal from "./Modal.svelte";
  import type { Page } from "./api";

  let {
    page,
    onconfirm,
    onclose,
  }: {
    page: Page;
    onconfirm: (selected: Page["links"]) => void;
    onclose: () => void;
  } = $props();

  // Main parts are selected, optional packs (languages, bonus) are not.
  let selected = $state<boolean[]>(untrack(() => page.links.map((l) => !l.optional)));
  const count = $derived(selected.filter(Boolean).length);
  const optionalCount = $derived(page.links.filter((l) => l.optional).length);

  function setAll(value: boolean, onlyOptional = false) {
    selected = page.links.map((l, i) => (onlyOptional && !l.optional ? selected[i] : value));
  }
</script>

<Modal
  title={page.game ?? page.title ?? "Add downloads"}
  subtitle={`${page.links.length} files found${optionalCount ? `, including ${optionalCount} optional` : ""}`}
  width={680}
  {onclose}
>
  <div class="tools">
    <button class="link" onclick={() => setAll(true)}>Select all</button>
    <button class="link" onclick={() => setAll(false)}>Select none</button>
    {#if optionalCount}
      <button class="link" onclick={() => setAll(true, true)}>Add optional files</button>
    {/if}
  </div>
  <ul>
    {#each page.links as link, i (link.url)}
      <li>
        <label>
          <input type="checkbox" bind:checked={selected[i]} />
          <span class="name" title={link.filename}>{link.filename}</span>
          {#if link.optional}<span class="tag">optional</span>{/if}
        </label>
      </li>
    {/each}
  </ul>

  {#snippet footer()}
    <span class="muted count">{count} of {page.links.length} selected</span>
    <button class="btn" onclick={onclose}>Cancel</button>
    <button
      class="btn primary"
      disabled={count === 0}
      onclick={() => onconfirm(page.links.filter((_, i) => selected[i]))}
    >
      Download {count} file{count === 1 ? "" : "s"}
    </button>
  {/snippet}
</Modal>

<style>
  .tools {
    display: flex;
    gap: 14px;
    margin-bottom: 8px;
  }
  .link {
    border: 0;
    padding: 0;
    background: none;
    color: var(--accent);
    font-size: 13px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    max-height: 52vh;
    overflow: auto;
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
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--mono);
    font-size: 12.5px;
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
  .count {
    margin-right: auto;
    font-size: 13px;
  }
</style>
