<script lang="ts">
  import type { Component } from 'svelte';
  import FolderSync from '@lucide/svelte/icons/folder-sync';
  import FolderPlus from '@lucide/svelte/icons/folder-plus';
  import Monitor from '@lucide/svelte/icons/monitor';
  import SettingsIcon from '@lucide/svelte/icons/settings';
  import icon from '../../../assets/branding/icon.svg';
  import { t } from '../lib/i18n.svelte';
  import type { Section } from '../lib/nav';

  let { section = $bindable() }: { section: Section } = $props();

  const top: { id: Section; label: string; icon: Component }[] = [
    { id: 'mounts', label: 'nav-mounts', icon: FolderSync },
    { id: 'add', label: 'nav-add', icon: FolderPlus },
  ];
  const bottom: { id: Section; label: string; icon: Component }[] = [
    { id: 'system', label: 'nav-system', icon: Monitor },
    { id: 'settings', label: 'nav-settings', icon: SettingsIcon },
  ];
</script>

<aside class="sidebar">
  <div class="brand">
    <img src={icon} alt="" width="36" height="36" />
    <strong>Syno Mount</strong>
  </div>

  {#snippet item(entry: { id: Section; label: string; icon: Component })}
    <button
      class="nav-item"
      aria-current={section === entry.id ? 'page' : undefined}
      title={t(entry.label)}
      onclick={() => (section = entry.id)}
    >
      <entry.icon size={18} strokeWidth={1.8} aria-hidden="true" />
      <span class="label">{t(entry.label)}</span>
    </button>
  {/snippet}

  <nav aria-label={t('nav-label')}>
    {#each top as entry (entry.id)}{@render item(entry)}{/each}
  </nav>
  <div class="bottom">
    {#each bottom as entry (entry.id)}{@render item(entry)}{/each}
  </div>
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 16px 10px;
    background: var(--sidebar);
    border-right: 1px solid var(--card-border);
    overflow-y: auto;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 8px 8px;
  }

  nav,
  .bottom {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .bottom {
    margin-top: auto;
  }

  .nav-item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: var(--control-height);
    padding: 0 10px;
    border: none;
    border-radius: var(--nav-radius);
    background: transparent;
    color: var(--text-2);
    text-align: left;
    cursor: pointer;
  }

  .nav-item:hover {
    background: var(--hover);
    color: var(--text);
  }

  .nav-item[aria-current='page'] {
    background: var(--selected);
    color: var(--text);
    font-weight: 600;
  }

  /* Fluent marks the selected item with an accent pill. */
  .nav-item[aria-current='page']::before {
    content: '';
    position: absolute;
    left: 0;
    top: 50%;
    width: 3px;
    height: 16px;
    border-radius: 3px;
    background: var(--accent);
    transform: translateY(-50%);
    opacity: var(--nav-indicator);
  }

  :global([data-skin='macos']) .nav-item[aria-current='page'] {
    background: var(--accent);
    color: var(--accent-text);
  }

  .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  @media (max-width: 720px) {
    .brand strong,
    .label {
      display: none;
    }

    .brand,
    .nav-item {
      justify-content: center;
      padding: 0;
    }
  }
</style>
