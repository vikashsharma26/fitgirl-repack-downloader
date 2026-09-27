<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";

  let {
    title,
    subtitle = "",
    width = 560,
    onclose,
    children,
    footer,
  }: {
    title: string;
    subtitle?: string;
    width?: number;
    onclose: () => void;
    children: Snippet;
    footer?: Snippet;
  } = $props();

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

<svelte:window {onkeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="modal" role="dialog" aria-modal="true" aria-label={title} style="width: min({width}px, 100% - 32px)">
    <header>
      <div>
        <h2>{title}</h2>
        {#if subtitle}<p class="muted">{subtitle}</p>{/if}
      </div>
      <button class="icon-btn" onclick={onclose} aria-label="Close"><Icon name="x" /></button>
    </header>
    <div class="body">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    background: rgb(8 10 16 / 0.45);
    backdrop-filter: blur(2px);
    animation: fade 0.12s ease-out;
  }
  .modal {
    display: flex;
    flex-direction: column;
    max-height: calc(100% - 48px);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 14px;
    box-shadow: var(--shadow);
    animation: rise 0.16s ease-out;
  }
  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding: 18px 20px 12px;
  }
  h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 650;
  }
  header p {
    margin: 2px 0 0;
    font-size: 13px;
  }
  .body {
    padding: 4px 20px 16px;
    overflow: auto;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 8px;
    padding: 14px 20px;
    border-top: 1px solid var(--border);
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  @keyframes rise {
    from {
      transform: translateY(8px);
      opacity: 0;
    }
  }
</style>
