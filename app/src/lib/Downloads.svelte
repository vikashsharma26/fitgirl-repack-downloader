<script lang="ts">
  import * as api from "./api";
  import type { ItemView, Totals } from "./api";
  import { bytes, eta, percent, speed } from "./format";
  import Icon from "./Icon.svelte";
  import RemoveDialog from "./RemoveDialog.svelte";

  type Filter = "all" | "active" | "completed" | "failed";

  let {
    items,
    totals,
    refresh,
    notify,
    onopengame,
  }: {
    items: ItemView[];
    totals: Totals;
    refresh: () => void;
    notify: (text: string, bad?: boolean) => void;
    onopengame: (slugOrUrl: string) => void;
  } = $props();

  let filter = $state<Filter>("all");
  let urlInput = $state("");
  let busy = $state(false);
  let collapsed = $state<Record<string, boolean>>({});
  let removing = $state<ItemView[] | null>(null);

  const STATUS_LABEL: Record<api.Status, string> = {
    queued: "Queued",
    resolving: "Getting link",
    downloading: "Downloading",
    paused: "Paused",
    completed: "Done",
    failed: "Failed",
  };

  const visible = $derived(
    items.filter((i) => {
      if (filter === "active") return !["completed", "failed"].includes(i.status);
      if (filter === "completed") return i.status === "completed";
      if (filter === "failed") return i.status === "failed";
      return true;
    }),
  );

  const groups = $derived.by(() => {
    const map = new Map<string, ItemView[]>();
    for (const item of visible) {
      const key = item.game ?? "Other downloads";
      if (!map.has(key)) map.set(key, []);
      map.get(key)!.push(item);
    }
    return [...map].map(([name, list]) => {
      const size = list.reduce((s, i) => s + (i.size ?? 0), 0);
      const known = list.every((i) => i.size != null);
      const done = list.reduce((s, i) => s + i.downloaded, 0);
      const rate = list.reduce((s, i) => s + i.speed, 0);
      return {
        name,
        list,
        size: known ? size : null,
        done,
        speed: rate,
        eta: known && rate > 0 ? (size - done) / rate : null,
        completed: list.filter((i) => i.status === "completed").length,
        running: list.some((i) => ["downloading", "resolving", "queued"].includes(i.status)),
      };
    });
  });

  async function add() {
    const urls = urlInput.split(/\s+/).filter((u) => /^https?:\/\//i.test(u));
    if (!urls.length) {
      notify("Paste a FitGirl game page, a fuckingfast.co link or any direct download link.", true);
      return;
    }
    busy = true;
    try {
      const pages = urls.filter(api.isGamePage);
      const direct = urls.filter((u) => !api.isGamePage(u));
      if (direct.length) {
        const r = await api.addLinks(direct.map((url) => ({ url })), null);
        notify(`Added ${r.added} download${r.added === 1 ? "" : "s"}${r.skipped ? ` (${r.skipped} already in the list)` : ""}.`);
      }
      urlInput = "";
      refresh();
      // Game pages open in the game view, where files can be picked.
      if (pages.length) onopengame(pages[0]);
    } catch (e) {
      notify(String(e), true);
    } finally {
      busy = false;
    }
  }

  async function confirmRemove(deleteFiles: boolean) {
    for (const item of removing ?? []) await api.remove(item.id, deleteFiles);
    removing = null;
    refresh();
  }

  async function groupAction(list: ItemView[], action: "pause" | "resume") {
    for (const i of list) {
      if (action === "pause" && ["queued", "resolving", "downloading"].includes(i.status)) await api.pause(i.id);
      if (action === "resume" && ["paused", "failed"].includes(i.status)) await api.resume(i.id);
    }
    refresh();
  }

  function act(fn: () => Promise<unknown>) {
    return () => fn().then(refresh).catch((e) => notify(String(e), true));
  }
</script>

<div class="view">
  <header class="top">
    <h1 class="title">Downloads</h1>
    <form
      class="add"
      onsubmit={(e) => {
        e.preventDefault();
        add();
      }}
    >
      <span class="add-icon"><Icon name="link" /></span>
      <input
        class="text-input"
        placeholder="Paste a FitGirl game page, fuckingfast.co link or direct URL"
        bind:value={urlInput}
        spellcheck="false"
      />
      <button class="btn primary" disabled={busy || !urlInput.trim()}>
        <Icon name="plus" />{busy ? "Reading…" : "Add"}
      </button>
    </form>
  </header>

  <nav class="toolbar">
    <div class="tabs" role="tablist">
      {#each [["all", "All", items.length], ["active", "Active", totals.active + totals.queued], ["completed", "Completed", totals.completed], ["failed", "Failed", totals.failed]] as [key, label, count] (key)}
        <button role="tab" class:on={filter === key} aria-selected={filter === key} onclick={() => (filter = key as Filter)}>
          {label}<span class="count">{count}</span>
        </button>
      {/each}
    </div>
    <div class="actions">
      <button class="btn" onclick={act(api.resumeAll)}><Icon name="play" />Resume all</button>
      <button class="btn" onclick={act(api.pauseAll)}><Icon name="pause" />Pause all</button>
      <button class="btn" onclick={act(api.clearCompleted)} disabled={!totals.completed}><Icon name="broom" />Clear completed</button>
    </div>
  </nav>

  <main>
    {#if !items.length}
      <div class="empty">
        <div class="logo big"><Icon name="download" /></div>
        <h1>Nothing downloading yet</h1>
        <p class="muted">
          Find a game in <strong>Discover</strong>, paste a FitGirl game page above, or click the
          <strong>FitDL</strong> button on a game page in your browser. Every fuckingfast.co file on the page is queued and downloaded with several
          connections, and resumes automatically if something goes wrong.
        </p>
      </div>
    {:else if !visible.length}
      <div class="empty"><p class="muted">No downloads in this view.</p></div>
    {:else}
      {#each groups as g (g.name)}
        <section class="group">
          <div class="group-head">
            <button class="icon-btn chev" class:open={!collapsed[g.name]} onclick={() => (collapsed[g.name] = !collapsed[g.name])} aria-label="Toggle">
              <Icon name="chevron" />
            </button>
            <div class="g-title">
              <strong title={g.name}>{g.name}</strong>
              <span class="muted num">
                {g.completed}/{g.list.length} files · {bytes(g.done)}{g.size != null ? ` of ${bytes(g.size)}` : ""}
                {#if g.speed}· {speed(g.speed)}{/if}
                {#if g.eta}· {eta(g.eta)} left{/if}
              </span>
            </div>
            <div class="bar g-bar" class:completed={g.completed === g.list.length}>
              <span style="width: {percent(g.done, g.size)}%"></span>
            </div>
            <div class="row-actions">
              {#if g.running}
                <button class="icon-btn" title="Pause game" onclick={() => groupAction(g.list, "pause")}><Icon name="pause" /></button>
              {:else}
                <button class="icon-btn" title="Resume game" onclick={() => groupAction(g.list, "resume")}><Icon name="play" /></button>
              {/if}
              <button class="icon-btn" title="Open folder" onclick={act(() => api.showItem(g.list[0].id))}><Icon name="folder" /></button>
              <button class="icon-btn" title="Remove game" onclick={() => (removing = g.list)}><Icon name="trash" /></button>
            </div>
          </div>

          {#if !collapsed[g.name]}
            <div class="table" role="table">
              {#each g.list as item (item.id)}
                <div class="tr" role="row">
                  <div class="name" role="cell">
                    <span class="file" title={item.filename}
                      ><span class="head">{item.filename.slice(0, -14)}</span><span class="tail">{item.filename.slice(-14)}</span></span
                    >
                    {#if item.error}<span class="err" title={item.error}>{item.error}</span>{/if}
                  </div>
                  <div class="progress" role="cell">
                    <div class="bar {item.status}"><span style="width: {percent(item.downloaded, item.size)}%"></span></div>
                    <span class="muted num small">{bytes(item.downloaded)} / {bytes(item.size)}</span>
                  </div>
                  <div class="num rate" role="cell">
                    {#if item.status === "downloading"}
                      <span>{speed(item.speed)}</span>
                      <span class="muted small">{eta(item.eta_secs)}{item.connections ? ` · ${item.connections} conn` : ""}</span>
                    {/if}
                  </div>
                  <div role="cell"><span class="chip {item.status}">{STATUS_LABEL[item.status]}</span></div>
                  <div class="row-actions" role="cell">
                    {#if ["queued", "resolving", "downloading"].includes(item.status)}
                      <button class="icon-btn" title="Pause" onclick={act(() => api.pause(item.id))}><Icon name="pause" /></button>
                    {:else if item.status === "paused"}
                      <button class="icon-btn" title="Resume" onclick={act(() => api.resume(item.id))}><Icon name="play" /></button>
                    {:else if item.status === "failed"}
                      <button class="icon-btn" title="Retry" onclick={act(() => api.resume(item.id))}><Icon name="retry" /></button>
                    {:else}
                      <span class="spacer"></span>
                    {/if}
                    <button class="icon-btn" title="Show in folder" onclick={act(() => api.showItem(item.id))}><Icon name="folder" /></button>
                    <button class="icon-btn" title="Remove" onclick={() => (removing = [item])}><Icon name="trash" /></button>
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </section>
      {/each}
    {/if}
  </main>
</div>

{#if removing}
  <RemoveDialog names={removing.map((i) => i.filename)} onconfirm={confirmRemove} onclose={() => (removing = null)} />
{/if}

<style>
  .view {
    display: grid;
    grid-template-rows: auto auto 1fr;
    height: 100%;
    min-height: 0;
  }
  .title {
    margin: 0;
    font-size: 18px;
    font-weight: 650;
    flex: none;
  }


  .top {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 12px 18px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  .logo {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: 9px;
    background: linear-gradient(135deg, #6d5dfc, #1fb6d9);
    color: #fff;
  }
  .logo :global(svg) {
    width: 28px;
    height: 28px;
    stroke-width: 2.6;
  }
  .logo.big {
    width: 56px;
    height: 56px;
    border-radius: 15px;
  }

  .add {
    flex: 1;
    display: flex;
    gap: 8px;
    position: relative;
    min-width: 0;
  }
  .add .text-input {
    flex: 1;
    padding-left: 34px;
  }
  .add-icon {
    position: absolute;
    left: 10px;
    top: 9px;
    color: var(--faint);
  }
  .add-icon :global(svg) {
    width: 16px;
    height: 16px;
  }

  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 18px;
    flex-wrap: wrap;
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
    font-variant-numeric: tabular-nums;
  }
  .actions {
    display: flex;
    gap: 8px;
  }

  main {
    overflow: auto;
    padding: 0 18px 18px;
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    max-width: 520px;
    margin: 12vh auto 0;
  }
  .empty h1 {
    margin: 16px 0 6px;
    font-size: 19px;
  }
  .empty p {
    margin: 0;
  }

  .group {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    margin-top: 10px;
    overflow: hidden;
  }
  .group-head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px 10px 8px;
  }
  .chev :global(svg) {
    transition: transform 0.15s;
  }
  .chev.open :global(svg) {
    transform: rotate(90deg);
  }
  .g-title {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .g-title strong {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .g-title span {
    font-size: 12.5px;
  }
  .g-bar {
    width: 22%;
    min-width: 120px;
    flex: none;
  }

  .table {
    border-top: 1px solid var(--border);
  }
  .tr {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(150px, 22%) 120px 104px auto;
    align-items: center;
    gap: 14px;
    padding: 8px 12px 8px 46px;
  }
  .tr + .tr {
    border-top: 1px solid var(--border);
  }
  .tr:hover {
    background: var(--surface-2);
  }
  .name {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  /* Truncate in the middle so the ".part07.rar" end stays visible. */
  .file {
    display: flex;
    min-width: 0;
    white-space: nowrap;
    font-family: var(--mono);
    font-size: 12.5px;
  }
  .file .head {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .file .tail {
    flex: none;
  }
  .err {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--bad);
    font-size: 12px;
  }
  .progress {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .rate {
    display: flex;
    flex-direction: column;
    font-size: 13px;
    line-height: 1.3;
  }
  .small {
    font-size: 11.5px;
  }
  .chip {
    display: inline-block;
    padding: 2px 9px;
    border-radius: 99px;
    font-size: 12px;
    font-weight: 600;
    background: var(--surface-2);
    color: var(--muted);
  }
  .chip.downloading,
  .chip.resolving {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .chip.completed {
    background: color-mix(in srgb, var(--ok) 14%, transparent);
    color: var(--ok);
  }
  .chip.failed {
    background: color-mix(in srgb, var(--bad) 14%, transparent);
    color: var(--bad);
  }
  .row-actions {
    display: flex;
    gap: 2px;
    justify-content: flex-end;
  }
  .spacer {
    width: 30px;
  }



  @media (max-width: 980px) {
    .tr {
      grid-template-columns: minmax(0, 1fr) 150px 96px auto;
    }
    .rate {
      display: none;
    }
    .actions .btn {
      padding: 0 10px;
    }
  }
</style>
