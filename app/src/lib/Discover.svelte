<script lang="ts">
  import { onMount, untrack } from "svelte";
  import * as api from "./api";
  import type { ItemView, Section, GameSummary } from "./api";
  import { folderName, imageFallback } from "./format";
  import Icon from "./Icon.svelte";

  let {
    items,
    reloadKey,
    onopen,
  }: {
    items: ItemView[];
    /** Changes when settings change, so filtered lists are reloaded. */
    reloadKey: number;
    onopen: (slug: string) => void;
  } = $props();

  type Tile = { slug: string; title: string; name: string; sub: string | null; cover: string | null; meta: string | null };

  let sections = $state<Section[] | null>(null);
  let popularError = $state<string | null>(null);
  let tab = $state("Today");

  let query = $state("");
  let active = $state<string | null>(null); // query whose results are shown
  let results = $state<GameSummary[]>([]);
  let total = $state<number | null>(null);
  let page = $state(1);
  let pages = $state(0);
  let searching = $state(false);
  let searchError = $state<string | null>(null);

  // Download state per game folder, for the "In downloads" / "Downloaded" badges.
  const inQueue = $derived.by(() => {
    const m = new Map<string, { total: number; done: number }>();
    for (const i of items) {
      if (!i.game) continue;
      const e = m.get(i.game) ?? { total: 0, done: 0 };
      e.total++;
      if (i.status === "completed") e.done++;
      m.set(i.game, e);
    }
    return m;
  });

  function badge(title: string): string | null {
    const e = inQueue.get(folderName(title));
    if (!e) return null;
    return e.done === e.total ? "Downloaded" : "In downloads";
  }

  const tiles = $derived.by((): Tile[] => {
    if (active != null) {
      return results.map((r) => ({
        slug: r.slug,
        title: r.title,
        name: r.name,
        sub: r.version,
        cover: r.cover,
        meta: [r.repack_size, r.genres.slice(0, 3).join(", ")].filter(Boolean).join(" · ") || null,
      }));
    }
    const s = sections?.find((s) => s.name === tab) ?? sections?.[0];
    return (s?.cards ?? []).map((c) => {
      const [name, ...rest] = c.title.split(/ [–—-] /);
      return { slug: c.slug, title: c.title, name, sub: rest.join(" – ") || null, cover: c.cover, meta: null };
    });
  });

  async function loadPopular() {
    popularError = null;
    try {
      sections = await api.popular();
      if (!sections.some((s) => s.name === tab) && sections.length) tab = sections[0].name;
    } catch (e) {
      popularError = String(e);
    }
  }

  async function runSearch(more = false) {
    const q = more ? active! : query.trim();
    if (!q) return clearSearch();
    searching = true;
    searchError = null;
    try {
      const next = more ? page + 1 : 1;
      const r = await api.search(q, next);
      results = more ? [...results, ...r.results.filter((x) => !results.some((y) => y.slug === x.slug))] : r.results;
      active = q;
      page = next;
      pages = r.total_pages;
      total = r.total;
    } catch (e) {
      searchError = String(e);
    } finally {
      searching = false;
    }
  }

  function clearSearch() {
    query = "";
    active = null;
    results = [];
    searchError = null;
  }

  onMount(loadPopular);

  let lastKey = untrack(() => reloadKey);
  $effect(() => {
    if (reloadKey !== lastKey) {
      lastKey = reloadKey;
      sections = null;
      loadPopular();
      if (active) runSearch();
    }
  });
</script>

<div class="discover">
  <header>
    <h1>Discover</h1>
    <form
      class="search"
      onsubmit={(e) => {
        e.preventDefault();
        runSearch();
      }}
    >
      <span class="search-icon"><Icon name="search" /></span>
      <input class="text-input" placeholder="Search FitGirl Repacks…" bind:value={query} spellcheck="false" />
      {#if active != null}
        <button type="button" class="icon-btn clear" title="Clear search" onclick={clearSearch}><Icon name="x" /></button>
      {/if}
      <button class="btn primary" disabled={searching || !query.trim()}>Search</button>
    </form>
  </header>

  <div class="scroll">
    {#if active != null}
      <div class="bar-row">
        <h2>
          Results for “{active}”
          {#if total != null}<span class="muted">· {total} post{total === 1 ? "" : "s"}</span>{/if}
        </h2>
      </div>
    {:else if sections && sections.length}
      <div class="bar-row">
        <div class="tabs" role="tablist">
          {#each sections as s (s.name)}
            <button role="tab" class:on={tab === s.name} aria-selected={tab === s.name} onclick={() => (tab = s.name)}>
              {s.name}<span class="count">{s.cards.length}</span>
            </button>
          {/each}
        </div>
        <span class="muted small">Most popular repacks on fitgirl-repacks.site</span>
      </div>
    {/if}

    {#if (active == null && popularError) || searchError}
      <div class="state">
        <p class="bad">{searchError ?? popularError}</p>
        <button class="btn" onclick={() => (searchError ? runSearch() : loadPopular())}><Icon name="retry" />Try again</button>
      </div>
    {:else if (active == null && !sections) || (searching && !results.length && active == null)}
      <div class="grid">
        {#each Array(12) as _, i (i)}
          <div class="card skeleton"><div class="cover"></div><div class="line"></div><div class="line short"></div></div>
        {/each}
      </div>
    {:else if active != null && !results.length && !searching}
      <div class="state"><p class="muted">No games found for “{active}”.</p></div>
    {:else}
      <div class="grid">
        {#each tiles as t (t.slug)}
          {@const b = badge(t.title)}
          <button class="card" onclick={() => onopen(t.slug)} title={t.title}>
            <div class="cover">
              {#if t.cover}
                <img src={t.cover} alt="" loading="lazy" decoding="async" onerror={imageFallback} />
              {:else}
                <span class="ph"><Icon name="image" /></span>
              {/if}
              {#if b}<span class="badge" class:done={b === "Downloaded"}><Icon name={b === "Downloaded" ? "check" : "download"} />{b}</span>{/if}
            </div>
            <div class="name">{t.name}</div>
            {#if t.sub}<div class="sub">{t.sub}</div>{/if}
            {#if t.meta}<div class="meta">{t.meta}</div>{/if}
          </button>
        {/each}
      </div>
      {#if active != null && page < pages}
        <div class="more">
          <button class="btn" disabled={searching} onclick={() => runSearch(true)}>{searching ? "Loading…" : "Load more"}</button>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .discover {
    display: grid;
    grid-template-rows: auto 1fr;
    height: 100%;
    min-height: 0;
  }
  header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 12px 18px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  h1 {
    margin: 0;
    font-size: 18px;
    font-weight: 650;
    flex: none;
  }
  .search {
    flex: 1;
    display: flex;
    gap: 8px;
    position: relative;
    max-width: 720px;
  }
  .search .text-input {
    flex: 1;
    padding-left: 34px;
    padding-right: 36px;
  }
  .search-icon {
    position: absolute;
    left: 10px;
    top: 9px;
    color: var(--faint);
  }
  .search-icon :global(svg) {
    width: 16px;
    height: 16px;
  }
  .clear {
    position: absolute;
    right: 88px;
    top: 2px;
  }
  .scroll {
    overflow: auto;
    padding: 4px 18px 24px;
  }
  .bar-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    padding: 10px 0;
  }
  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  .small {
    font-size: 12.5px;
  }
  .tabs {
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: 9px;
    background: var(--surface-2);
  }
  .tabs button {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 12px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--muted);
  }
  .tabs button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.08);
  }
  .count {
    font-size: 11.5px;
    color: var(--faint);
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(156px, 1fr));
    gap: 18px 16px;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 0;
    border: 0;
    background: none;
    text-align: left;
    min-width: 0;
  }
  .cover {
    position: relative;
    aspect-ratio: 3 / 4;
    border-radius: 10px;
    overflow: hidden;
    background: var(--surface-2);
    border: 1px solid var(--border);
    margin-bottom: 6px;
    transition: transform 0.15s ease, box-shadow 0.15s ease;
  }
  .card:hover .cover,
  .card:focus-visible .cover {
    transform: translateY(-3px);
    box-shadow: var(--shadow);
  }
  .card:focus-visible {
    outline: none;
  }
  .card:focus-visible .cover {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .ph {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--faint);
  }
  .ph :global(svg) {
    width: 32px;
    height: 32px;
  }
  .badge {
    position: absolute;
    left: 6px;
    bottom: 6px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px 2px 6px;
    border-radius: 99px;
    background: var(--accent);
    color: #fff;
    font-size: 11px;
    font-weight: 600;
  }
  .badge.done {
    background: var(--ok);
  }
  .badge :global(svg) {
    width: 12px;
    height: 12px;
    stroke-width: 3;
  }
  .name {
    font-weight: 600;
    font-size: 13.5px;
    line-height: 1.3;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .sub,
  .meta {
    font-size: 12px;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta {
    color: var(--faint);
  }

  .skeleton {
    pointer-events: none;
  }
  .skeleton .cover,
  .skeleton .line {
    background: linear-gradient(90deg, var(--surface-2), var(--border), var(--surface-2));
    background-size: 200% 100%;
    animation: shimmer 1.2s linear infinite;
  }
  .skeleton .line {
    height: 12px;
    border-radius: 6px;
    margin-top: 4px;
  }
  .skeleton .line.short {
    width: 60%;
  }
  @keyframes shimmer {
    to {
      background-position: -200% 0;
    }
  }

  .state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 12vh 0;
  }
  .bad {
    color: var(--bad);
    margin: 0;
  }
  .more {
    display: flex;
    justify-content: center;
    padding-top: 22px;
  }
</style>
