<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    open = $bindable(),
    title,
    wide = false,
    children,
    actions,
  }: {
    open: boolean;
    title: string;
    wide?: boolean;
    children: Snippet;
    actions?: Snippet;
  } = $props();

  let dialog: HTMLDialogElement;
  const id = `dialog-${Math.random().toString(36).slice(2)}`;

  // A modal <dialog>; system dialogs do not work in every WebView.
  $effect(() => {
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  });
</script>

<dialog bind:this={dialog} class:wide onclose={() => (open = false)} aria-labelledby={id}>
  {#if open}
    <h2 {id}>{title}</h2>
    <div class="body">{@render children()}</div>
    {#if actions}<div class="actions">{@render actions()}</div>{/if}
  {/if}
</dialog>

<style>
  dialog {
    width: min(480px, calc(100vw - 32px));
    max-height: calc(100vh - 48px);
    padding: 22px 24px;
    border: none;
    border-radius: var(--radius-card);
    background: var(--card);
    color: var(--text);
    box-shadow:
      0 0 0 1px var(--card-border),
      0 20px 50px rgb(0 0 0 / 0.3);
  }

  dialog.wide {
    width: min(680px, calc(100vw - 32px));
  }

  dialog::backdrop {
    background: rgb(0 0 0 / 0.35);
  }

  dialog[open] {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 12px;
    overflow-y: auto;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
