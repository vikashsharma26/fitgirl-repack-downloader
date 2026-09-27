<script lang="ts">
  import Modal from "./Modal.svelte";

  let {
    names,
    onconfirm,
    onclose,
  }: {
    names: string[];
    onconfirm: (deleteFiles: boolean) => void;
    onclose: () => void;
  } = $props();

  let deleteFiles = $state(false);
</script>

<Modal
  title={names.length === 1 ? "Remove download?" : `Remove ${names.length} downloads?`}
  subtitle={names.length === 1 ? names[0] : ""}
  width={460}
  {onclose}
>
  <label>
    <input type="checkbox" bind:checked={deleteFiles} />
    Also delete the downloaded data from disk
  </label>
  {#snippet footer()}
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn primary" onclick={() => onconfirm(deleteFiles)}>Remove</button>
  {/snippet}
</Modal>

<style>
  label {
    display: flex;
    gap: 10px;
    align-items: center;
    cursor: pointer;
  }
  input {
    accent-color: var(--accent);
    width: 15px;
    height: 15px;
  }
</style>
