<script lang="ts">
  import { untrack } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import Modal from "./Modal.svelte";
  import type { Config } from "./api";

  let {
    config,
    configPath,
    onsave,
    onclose,
  }: {
    config: Config;
    configPath: string;
    onsave: (config: Config) => Promise<void>;
    onclose: () => void;
  } = $props();

  // Edit a copy; the saved config only changes when the user clicks Save.
  let draft = $state<Config>(untrack(() => structuredClone($state.snapshot(config))));
  let saving = $state(false);
  const portChanged = $derived(draft.server_port !== config.server_port);

  async function browse() {
    const dir = await open({ directory: true, defaultPath: draft.download_dir });
    if (typeof dir === "string") draft.download_dir = dir;
  }

  async function save() {
    saving = true;
    try {
      await onsave($state.snapshot(draft));
    } finally {
      saving = false;
    }
  }
</script>

<Modal title="Settings" subtitle={configPath} width={600} {onclose}>
  <section>
    <h3>Downloads</h3>
    <div class="field">
      <span>Download folder</span>
      <div class="row">
        <input class="text-input grow" bind:value={draft.download_dir} spellcheck="false" />
        <button class="btn" onclick={browse}>Browse…</button>
      </div>
    </div>
    <label class="check">
      <input type="checkbox" bind:checked={draft.subfolder_per_game} />
      Put each game in its own folder
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={draft.hide_adult} />
      Hide adult (18+) games in Discover
    </label>
  </section>

  <section>
    <h3>Speed</h3>
    <div class="grid">
      <label class="field">
        <span>Files at the same time</span>
        <input class="text-input" type="number" min="1" max="32" bind:value={draft.max_parallel_files} />
      </label>
      <label class="field">
        <span>Connections per file</span>
        <input class="text-input" type="number" min="1" max="32" bind:value={draft.connections_per_file} />
      </label>
      <label class="field">
        <span>Speed limit (KB/s, 0 = unlimited)</span>
        <input class="text-input" type="number" min="0" bind:value={draft.speed_limit_kbps} />
      </label>
    </div>
    <p class="hint">
      Many hosts cap speed per IP address, so more connections are not always faster. Try 2–4 files × 4
      connections.
    </p>
  </section>

  <details>
    <summary>Advanced</summary>
    <div class="grid">
      <label class="field">
        <span>Min. segment size (MB)</span>
        <input class="text-input" type="number" min="1" bind:value={draft.min_segment_size_mb} />
      </label>
      <label class="field">
        <span>Write buffer (KB)</span>
        <input class="text-input" type="number" min="16" bind:value={draft.buffer_size_kb} />
      </label>
      <label class="field">
        <span>Retries per segment</span>
        <input class="text-input" type="number" min="0" bind:value={draft.max_retries} />
      </label>
      <label class="field">
        <span>Retry delay (ms)</span>
        <input class="text-input" type="number" min="0" bind:value={draft.retry_backoff_ms} />
      </label>
      <label class="field">
        <span>Save progress every (s)</span>
        <input class="text-input" type="number" min="1" bind:value={draft.state_save_interval_s} />
      </label>
      <label class="field">
        <span>Extension port</span>
        <input class="text-input" type="number" min="1024" max="65535" bind:value={draft.server_port} />
      </label>
    </div>
    <label class="field">
      <span>User-Agent</span>
      <input class="text-input" bind:value={draft.user_agent} spellcheck="false" />
    </label>
    {#if portChanged}
      <p class="hint warn">Restart FitDL for the new port to take effect, and set the same port in the extension.</p>
    {/if}
  </details>

  {#snippet footer()}
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn primary" disabled={saving} onclick={save}>Save</button>
  {/snippet}
</Modal>

<style>
  section,
  details {
    padding: 12px 0;
  }
  section + section,
  details {
    border-top: 1px solid var(--border);
  }
  h3,
  summary {
    margin: 0 0 10px;
    font-size: 12px;
    font-weight: 650;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  summary {
    cursor: pointer;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
    gap: 12px;
    margin-bottom: 12px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-size: 13px;
  }
  .field > span {
    color: var(--muted);
  }
  .row {
    display: flex;
    gap: 8px;
  }
  .grow {
    flex: 1;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 12px;
    cursor: pointer;
  }
  .check input {
    accent-color: var(--accent);
    width: 15px;
    height: 15px;
  }
  .hint {
    margin: 0;
    font-size: 12.5px;
    color: var(--muted);
  }
  .warn {
    margin-top: 10px;
    color: var(--warn);
  }
</style>
