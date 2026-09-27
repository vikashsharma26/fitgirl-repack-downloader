<script lang="ts">
  import * as api from "./api";
  import type { GameDetails, ItemView } from "./api";
  import FileList from "./FileList.svelte";
  import { imageFallback } from "./format";
  import Icon from "./Icon.svelte";

  let {
    slug,
    items,
    onback,
    ondownload,
    notify,
  }: {
    slug: string;
    items: ItemView[];
    onback: () => void;
    ondownload: () => void;
    notify: (text: string, bad?: boolean) => void;
  } = $props();

  let game = $state<GameDetails | null>(null);
  let error = $state<string | null>(null);
  let selected = $state<boolean[]>([]);
  let adding = $state(false);
  let shot = $state<number | null>(null); // open screenshot in the lightbox
  let expanded = $state(false);

  const queued = $derived(new Set(items.map((i) => i.url)));
  const count = $derived(selected.filter(Boolean).length);
  const longDescription = $derived((game?.description?.length ?? 0) > 600);
  const alreadyQueued = $derived(game ? game.links.filter((l) => queued.has(l.url)).length : 0);

  async function load(s: string) {
    game = null;
    error = null;
    expanded = false;
    try {
      const g = await api.game(s);
      game = g;
      selected = g.links.map((l) => !l.optional);
    } catch (e) {
      error = String(e);
    }
  }

  $effect(() => {
    load(slug);
  });

  async function download() {
    if (!game) return;
    adding = true;
    try {
      const links = game.links
        .filter((_, i) => selected[i])
        .map((l) => ({ url: l.url, label: l.label, filename: l.filename }));
      const r = await api.addLinks(links, game.title);
      notify(
        r.added
          ? `Queued ${r.added} file${r.added === 1 ? "" : "s"} of ${game.name}${r.skipped ? ` (${r.skipped} already queued)` : ""}.`
          : "Those files are already in Downloads.",
      );
      ondownload();
    } catch (e) {
      notify(String(e), true);
    } finally {
      adding = false;
    }
  }

  function onkeydown(e: KeyboardEvent) {
    const current = shot;
    if (current == null || !game) return;
    const n = game.screenshots.length;
    if (e.key === "Escape") shot = null;
    if (e.key === "ArrowRight") shot = (current + 1) % n;
    if (e.key === "ArrowLeft") shot = (current - 1 + n) % n;
  }
</script>

<svelte:window {onkeydown} />

<div class="game">
  <header>
    <button class="btn" onclick={onback}><Icon name="back" />Back</button>
    {#if game}<span class="crumb muted">{game.name}</span>{/if}
  </header>

  <div class="scroll">
    {#if error}
      <div class="state">
        <p class="bad">{error}</p>
        <button class="btn" onclick={() => load(slug)}><Icon name="retry" />Try again</button>
      </div>
    {:else if !game}
      <div class="hero skeleton">
        <div class="cover"></div>
        <div class="info"><div class="line wide"></div><div class="line"></div><div class="line"></div></div>
      </div>
    {:else}
      <section class="hero">
        <div class="cover">
          {#if game.cover}<img src={game.cover} alt="" onerror={imageFallback} />{:else}<span class="ph"><Icon name="image" /></span>{/if}
        </div>
        <div class="info">
          <h1>{game.name}</h1>
          {#if game.version}<p class="version">{game.version}</p>{/if}

          <dl class="specs">
            {#if game.repack_size}<div><dt>Repack size</dt><dd>{game.repack_size}</dd></div>{/if}
            {#if game.original_size}<div><dt>Original size</dt><dd>{game.original_size}</dd></div>{/if}
            {#if game.languages}<div><dt>Languages</dt><dd>{game.languages}</dd></div>{/if}
            {#if game.date}<div><dt>Posted</dt><dd>{new Date(game.date).toLocaleDateString()}</dd></div>{/if}
            {#if game.companies}<div class="full"><dt>Companies</dt><dd>{game.companies}</dd></div>{/if}
          </dl>
          {#if game.genres.length}
            <div class="chips">{#each game.genres as g (g)}<span class="chip">{g}</span>{/each}</div>
          {/if}

          <div class="actions">
            {#if game.links.length}
              <button class="btn primary big" disabled={adding || count === 0} onclick={download}>
                <Icon name="download" />Download {count} file{count === 1 ? "" : "s"}
              </button>
            {/if}
            <button class="btn big" onclick={() => api.openUrl(game!.url).catch((e) => notify(String(e), true))}>
              <Icon name="external" />Open on website
            </button>
          </div>
          {#if !game.links.length}
            <p class="muted note">No fuckingfast.co links on this post, so FitDL can't download it.</p>
          {:else if alreadyQueued}
            <p class="muted note">{alreadyQueued} of {game.links.length} files are already in Downloads.</p>
          {/if}
        </div>
      </section>

      {#if game.screenshots.length}
        <section>
          <h2>Screenshots</h2>
          <div class="shots">
            {#each game.screenshots as s, i (s.full)}
              <button class="shot" onclick={() => (shot = i)} aria-label="Open screenshot {i + 1}">
                <img src={s.thumb} alt="" loading="lazy" />
              </button>
            {/each}
          </div>
        </section>
      {/if}

      {#if game.description}
        <section>
          <h2>About</h2>
          <p class="desc" class:clamped={longDescription && !expanded}>{game.description}</p>
          {#if longDescription}
            <button class="more" onclick={() => (expanded = !expanded)}>{expanded ? "Show less" : "Show more"}</button>
          {/if}
        </section>
      {/if}

      {#if game.links.length}
        <section>
          <h2>Files <span class="muted">· {game.links.length}</span></h2>
          <FileList links={game.links} bind:selected {queued} />
        </section>
      {/if}
    {/if}
  </div>
</div>

{#if shot != null && game}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="lightbox" onclick={(e) => e.target === e.currentTarget && (shot = null)}>
    <button class="nav prev" aria-label="Previous" onclick={() => (shot = (shot! - 1 + game!.screenshots.length) % game!.screenshots.length)}><Icon name="left" /></button>
    <img src={game.screenshots[shot].full} alt="Screenshot {shot + 1}" />
    <button class="nav next" aria-label="Next" onclick={() => (shot = (shot! + 1) % game!.screenshots.length)}><Icon name="right" /></button>
    <button class="nav close" aria-label="Close" onclick={() => (shot = null)}><Icon name="x" /></button>
  </div>
{/if}

<style>
  .game {
    display: grid;
    grid-template-rows: auto 1fr;
    height: 100%;
    min-height: 0;
  }
  header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 18px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  .crumb {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .scroll {
    overflow: auto;
    padding: 22px 24px 32px;
  }
  .scroll > * {
    max-width: 1080px;
    margin-left: auto;
    margin-right: auto;
  }
  section + section {
    margin-top: 28px;
  }
  h2 {
    margin: 0 0 10px;
    font-size: 15px;
    font-weight: 650;
  }

  .hero {
    display: grid;
    grid-template-columns: 220px 1fr;
    gap: 26px;
    align-items: start;
  }
  .cover {
    position: relative;
    aspect-ratio: 3 / 4;
    border-radius: 12px;
    overflow: hidden;
    background: var(--surface-2);
    border: 1px solid var(--border);
    box-shadow: var(--shadow);
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
    width: 40px;
    height: 40px;
  }
  h1 {
    margin: 2px 0 4px;
    font-size: 26px;
    line-height: 1.2;
    font-weight: 700;
    text-wrap: balance;
  }
  .version {
    margin: 0 0 16px;
    color: var(--muted);
    font-size: 14.5px;
  }
  .specs {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 12px 20px;
    margin: 0 0 14px;
  }
  .specs .full {
    grid-column: 1 / -1;
  }
  dt {
    font-size: 11.5px;
    font-weight: 650;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--faint);
  }
  dd {
    margin: 2px 0 0;
    font-weight: 500;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 18px;
  }
  .chip {
    padding: 3px 10px;
    border-radius: 99px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    font-size: 12.5px;
    color: var(--muted);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }
  .btn.big {
    height: 40px;
    padding: 0 18px;
  }
  .note {
    margin: 10px 0 0;
    font-size: 13px;
  }

  .shots {
    display: flex;
    gap: 10px;
    overflow-x: auto;
    padding-bottom: 6px;
  }
  .shot {
    flex: none;
    width: 240px;
    aspect-ratio: 16 / 9;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
    background: var(--surface-2);
  }
  .shot img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
    transition: transform 0.2s ease;
  }
  .shot:hover img {
    transform: scale(1.04);
  }
  .desc {
    margin: 0;
    white-space: pre-line;
    line-height: 1.6;
    max-width: 78ch;
  }

  .desc.clamped {
    display: -webkit-box;
    -webkit-line-clamp: 7;
    line-clamp: 7;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .more {
    margin-top: 6px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font-weight: 500;
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
  .skeleton .cover,
  .skeleton .line {
    background: linear-gradient(90deg, var(--surface-2), var(--border), var(--surface-2));
    background-size: 200% 100%;
    animation: shimmer 1.2s linear infinite;
    box-shadow: none;
  }
  .skeleton .line {
    height: 16px;
    width: 40%;
    border-radius: 8px;
    margin-bottom: 12px;
  }
  .skeleton .line.wide {
    height: 28px;
    width: 70%;
  }
  @keyframes shimmer {
    to {
      background-position: -200% 0;
    }
  }

  .lightbox {
    position: fixed;
    inset: 0;
    z-index: 70;
    display: grid;
    place-items: center;
    background: rgb(5 6 10 / 0.88);
    animation: fade 0.12s ease-out;
  }
  .lightbox img {
    max-width: calc(100% - 140px);
    max-height: calc(100% - 80px);
    border-radius: 8px;
    box-shadow: 0 10px 40px rgb(0 0 0 / 0.5);
  }
  .nav {
    position: absolute;
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border: 0;
    border-radius: 50%;
    background: rgb(255 255 255 / 0.12);
    color: #fff;
  }
  .nav:hover {
    background: rgb(255 255 255 / 0.22);
  }
  .nav :global(svg) {
    width: 22px;
    height: 22px;
  }
  .prev {
    left: 20px;
  }
  .next {
    right: 20px;
  }
  .close {
    top: 18px;
    right: 20px;
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }

  @media (max-width: 760px) {
    .hero {
      grid-template-columns: 1fr;
    }
    .cover {
      max-width: 220px;
    }
  }
</style>
