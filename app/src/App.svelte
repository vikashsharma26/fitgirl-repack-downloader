<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "./lib/api";
  import type { AppInfo, Config, ItemView, Page, Totals } from "./lib/api";
  import { bytes, eta, percent, speed } from "./lib/format";
  import Icon from "./lib/Icon.svelte";
  import AddDialog from "./lib/AddDialog.svelte";
  import SettingsDialog from "./lib/SettingsDialog.svelte";
  import RemoveDialog from "./lib/RemoveDialog.svelte";

  type Filter = "all" | "active" | "completed" | "failed";

  let items = $state<ItemView[]>([]);
  let totals = $state<Totals>({ active: 0, queued: 0, completed: 0, failed: 0, speed: 0 });
  let info = $state<AppInfo | null>(null);
  let filter = $state<Filter>("all");
  let urlInput = $state("");
  let busy = $state(false);
  let toast = $state<{ text: string; bad: boolean } | null>(null);
  let collapsed = $state<Record<string, boolean>>({});

  let page = $state<Page | null>(null);
  let config = $state<Config | null>(null);
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

  async function refresh() {
    try {
      const data = await api.getItems();
      items = data.items;
      totals = data.totals;
    } catch (e) {
      console.error(e);
    }
  }

  onMount(() => {
    refresh();
    api.appInfo().then((i) => (info = i));
    const fast = setInterval(refresh, 500);
    const slow = setInterval(() => api.appInfo().then((i) => (info = i)), 5000);
    return () => {
      clearInterval(fast);
      clearInterval(slow);
    };
  });

  function notify(text: string, bad = false) {
    toast = { text, bad };
    const current = toast;
    setTimeout(() => toast === current && (toast = null), 4500);
  }

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
      if (pages.length) {
        const scraped = await api.scrape(pages[0]);
        if (!scraped.links.length) notify("No fuckingfast.co links found on that page.", true);
        else page = scraped;
      }
      urlInput = "";
      refresh();
    } catch (e) {
      notify(String(e), true);
    } finally {
      busy = false;
    }
  }

  async function confirmAdd(links: Page["links"]) {
    if (!page) return;
    const r = await api.addLinks(
      links.map((l) => ({ url: l.url, label: l.label, filename: l.filename })),
      page.game ?? page.title,
    );
    notify(`Queued ${r.added} file${r.added === 1 ? "" : "s"}${r.skipped ? `, ${r.skipped} already in the list` : ""}.`);
    page = null;
    refresh();
  }

  async function openSettings() {
    config = await api.getConfig();
  }

  async function saveSettings(next: Config) {
    try {
      await api.setConfig(next);
      config = null;
      notify("Settings saved.");
    } catch (e) {
      notify(String(e), true);
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

<div class="shell">
  <header class="top">
    <div class="brand">
      <div class="logo"><Icon name="download" /></div>
      <div>
        <strong>FitDL</strong>
        <span class="muted">{totals.active ? `${totals.active} downloading` : totals.queued ? `${totals.queued} queued` : "Idle"}</span>
      </div>
    </div>

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

    <div class="speed num" title="Total download speed">
      <span>{totals.speed ? bytes(totals.speed) + "/s" : "0 B/s"}</span>
    </div>
    <button class="icon-btn" title="Settings" onclick={openSettings}><Icon name="settings" /></button>
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
          Paste a FitGirl game page above, or open one in your browser and click the <strong>FitDL</strong>
          extension button. Every fuckingfast.co file on the page is queued and downloaded with several
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

  <footer class="status">
    {#if info}
      <span class="dot" class:ok={info.api.running}></span>
      {#if info.api.running}
        Browser extension link active on 127.0.0.1:{info.api.port}
      {:else}
        <span class="bad" title={info.api.error ?? ""}>Browser extension link unavailable: {info.api.error}</span>
      {/if}
      <span class="grow"></span>
      <button class="plain" onclick={act(api.openDownloads)}>Open downloads folder</button>
      <span class="muted">v{info.version}</span>
    {/if}
  </footer>
</div>

{#if toast}
  <div class="toast" class:bad={toast.bad} role="status">{toast.text}</div>
{/if}

{#if page}
  <AddDialog {page} onconfirm={confirmAdd} onclose={() => (page = null)} />
{/if}
{#if config && info}
  <SettingsDialog {config} configPath={info.config_path} onsave={saveSettings} onclose={() => (config = null)} />
{/if}
{#if removing}
  <RemoveDialog names={removing.map((i) => i.filename)} onconfirm={confirmRemove} onclose={() => (removing = null)} />
{/if}

<style>
  .shell {
    display: grid;
    grid-template-rows: auto auto 1fr auto;
    height: 100%;
  }

  .top {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 12px 18px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: none;
  }
  .brand > div:last-child {
    display: flex;
    flex-direction: column;
    line-height: 1.2;
  }
  .brand strong {
    font-size: 15px;
  }
  .brand .muted {
    font-size: 12px;
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
    width: 19px;
    height: 19px;
    stroke-width: 2.6;
  }
  .logo.big {
    width: 56px;
    height: 56px;
    border-radius: 15px;
  }
  .logo.big :global(svg) {
    width: 28px;
    height: 28px;
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
  .speed {
    flex: none;
    min-width: 92px;
    text-align: right;
    font-weight: 600;
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

  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 18px;
    border-top: 1px solid var(--border);
    background: var(--surface);
    font-size: 12.5px;
    color: var(--muted);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--bad);
  }
  .dot.ok {
    background: var(--ok);
  }
  .bad {
    color: var(--bad);
  }
  .grow {
    flex: 1;
  }
  .plain {
    border: 0;
    background: none;
    padding: 0;
    color: var(--accent);
  }

  .toast {
    position: fixed;
    left: 50%;
    bottom: 48px;
    transform: translateX(-50%);
    max-width: min(640px, calc(100% - 32px));
    padding: 10px 16px;
    border-radius: 10px;
    background: var(--text);
    color: var(--bg);
    box-shadow: var(--shadow);
    font-size: 13.5px;
    z-index: 60;
    animation: pop 0.15s ease-out;
  }
  .toast.bad {
    background: var(--bad);
    color: #fff;
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translate(-50%, 6px);
    }
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
