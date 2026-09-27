<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "./lib/api";
  import type { AppInfo, Config, ItemView, Totals } from "./lib/api";
  import { bytes } from "./lib/format";
  import Icon from "./lib/Icon.svelte";
  import Discover from "./lib/Discover.svelte";
  import Downloads from "./lib/Downloads.svelte";
  import GameView from "./lib/GameView.svelte";
  import SettingsDialog from "./lib/SettingsDialog.svelte";

  type View = "discover" | "downloads";

  let view = $state<View>("discover");
  let gameSlug = $state<string | null>(null); // game page open on top of `view`
  let items = $state<ItemView[]>([]);
  let totals = $state<Totals>({ active: 0, queued: 0, completed: 0, failed: 0, speed: 0 });
  let info = $state<AppInfo | null>(null);
  let config = $state<Config | null>(null);
  let reloadKey = $state(0);
  let toast = $state<{ text: string; bad: boolean } | null>(null);

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

  function go(next: View) {
    view = next;
    gameSlug = null;
  }

  async function openSettings() {
    config = await api.getConfig();
  }

  async function saveSettings(next: Config) {
    try {
      await api.setConfig(next);
      config = null;
      reloadKey++;
      notify("Settings saved.");
    } catch (e) {
      notify(String(e), true);
    }
  }
</script>

<div class="app">
  <aside>
    <div class="brand">
      <div class="logo"><Icon name="download" /></div>
      <strong>FitDL</strong>
    </div>
    <nav>
      <button class:on={view === "discover"} onclick={() => go("discover")}>
        <Icon name="compass" />Discover
      </button>
      <button class:on={view === "downloads"} onclick={() => go("downloads")}>
        <Icon name="download" />Downloads
        {#if totals.active + totals.queued}<span class="pill">{totals.active + totals.queued}</span>{/if}
      </button>
    </nav>
    <div class="grow"></div>
    <div class="speed" title="Total download speed">
      <span class="muted">{totals.active ? `${totals.active} downloading` : totals.queued ? `${totals.queued} queued` : "Idle"}</span>
      <strong class="num">{totals.speed ? `${bytes(totals.speed)}/s` : "0 B/s"}</strong>
    </div>
    <button class="side-btn" onclick={openSettings}><Icon name="settings" />Settings</button>
  </aside>

  <div class="content">
    <main>
      <!-- Discover stays mounted so search results and scroll survive opening a game. -->
      <div class="pane" hidden={view !== "discover" || gameSlug != null}>
        <Discover {items} {reloadKey} onopen={(slug) => (gameSlug = slug)} />
      </div>
      {#if view === "downloads" && gameSlug == null}
        <div class="pane">
          <Downloads {items} {totals} {refresh} {notify} onopengame={(url) => (gameSlug = url)} />
        </div>
      {/if}
      {#if gameSlug != null}
        <div class="pane">
          <GameView
            slug={gameSlug}
            {items}
            {notify}
            onback={() => (gameSlug = null)}
            ondownload={() => {
              refresh();
              go("downloads");
            }}
          />
        </div>
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
        <button class="plain" onclick={() => api.openDownloads().catch((e) => notify(String(e), true))}>Open downloads folder</button>
        <span class="muted">v{info.version}</span>
      {/if}
    </footer>
  </div>
</div>

{#if toast}
  <div class="toast" class:bad={toast.bad} role="status">{toast.text}</div>
{/if}

{#if config && info}
  <SettingsDialog {config} configPath={info.config_path} onsave={saveSettings} onclose={() => (config = null)} />
{/if}

<style>
  .app {
    display: grid;
    grid-template-columns: 200px 1fr;
    height: 100%;
  }

  aside {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 14px 10px 12px;
    background: var(--surface);
    border-right: 1px solid var(--border);
    min-height: 0;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 6px 14px;
    font-size: 16px;
  }
  .logo {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 9px;
    background: linear-gradient(135deg, #6d5dfc, #1fb6d9);
    color: #fff;
  }
  .logo :global(svg) {
    width: 18px;
    height: 18px;
    stroke-width: 2.6;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  nav button,
  .side-btn {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 36px;
    padding: 0 10px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--muted);
    font-weight: 500;
    text-align: left;
  }
  nav button :global(svg),
  .side-btn :global(svg) {
    width: 18px;
    height: 18px;
  }
  nav button:hover,
  .side-btn:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  nav button.on {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .pill {
    margin-left: auto;
    min-width: 20px;
    padding: 0 6px;
    border-radius: 99px;
    background: var(--accent);
    color: #fff;
    font-size: 11.5px;
    font-weight: 700;
    text-align: center;
    line-height: 18px;
  }
  .speed {
    display: flex;
    flex-direction: column;
    padding: 10px;
    margin-bottom: 4px;
    border-radius: 8px;
    background: var(--surface-2);
    font-size: 12px;
  }
  .speed strong {
    font-size: 15px;
  }

  .content {
    display: grid;
    grid-template-rows: 1fr auto;
    min-width: 0;
    min-height: 0;
  }
  main {
    position: relative;
    min-height: 0;
  }
  .pane {
    position: absolute;
    inset: 0;
  }
  .pane[hidden] {
    display: none;
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
    z-index: 80;
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
</style>
