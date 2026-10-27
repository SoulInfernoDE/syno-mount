<script lang="ts">
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import Globe from '@lucide/svelte/icons/globe';
  import HardDrive from '@lucide/svelte/icons/hard-drive';
  import House from '@lucide/svelte/icons/house';
  import {
    openFolder,
    startMount,
    stopMount,
    toFailure,
    type Failure,
    type MountView,
  } from '../lib/backend';
  import { milliseconds } from '../lib/format';
  import { t } from '../lib/i18n.svelte';
  import type { MountAction } from '../lib/nav';

  let {
    view,
    onchanged,
    onaction,
  }: {
    view: MountView;
    onchanged: () => void;
    onaction: (action: MountAction) => void;
  } = $props();

  let busy = $state(false);
  let failure = $state<Failure | null>(null);
  let menu = $state(false);

  const mount = $derived(view.mount);
  const status = $derived(view.status);
  const active = $derived(['mounted', 'stale', 'connecting'].includes(status.state));
  const badge = $derived(
    status.state === 'mounted'
      ? 'success'
      : status.state === 'stopped'
        ? ''
        : status.state === 'connecting'
          ? 'warning'
          : 'danger',
  );

  async function run(job: () => Promise<void>) {
    busy = true;
    failure = null;
    try {
      await job();
    } catch (error) {
      failure = toFailure(error);
    } finally {
      busy = false;
      onchanged();
    }
  }

  function choose(action: MountAction) {
    menu = false;
    onaction(action);
  }

  const menuItems: { action: MountAction; label: string }[] = [
    { action: 'selftest', label: 'mount-selftest' },
    { action: 'speed', label: 'mount-speed' },
    { action: 'settings', label: 'mount-settings' },
    { action: 'command', label: 'mount-command' },
    { action: 'more', label: 'mount-more-shares' },
    { action: 'remove', label: 'mount-remove' },
  ];
</script>

<article class="card mount" aria-busy={busy}>
  <div class="head">
    <div class="icon"><HardDrive size={22} strokeWidth={1.6} aria-hidden="true" /></div>
    <div class="title">
      <h2>{t('mount-on', { share: mount.share, nas: mount.nasName })}</h2>
      <p class="subtle selectable">{mount.localPath}</p>
    </div>
    <span class="badge {badge}">{t(`state-${status.state}`)}</span>
  </div>

  <div class="facts subtle">
    {#if status.connected}
      <span class="fact">
        {#if status.connected.route === 'lan'}
          <House size={14} aria-hidden="true" />
        {:else}
          <Globe size={14} aria-hidden="true" />
        {/if}
        {t(`route-${status.connected.route}`)} · {status.connected.address.host}:{status.connected
          .address.port}
      </span>
    {/if}
    {#if status.latencyMs !== undefined}
      <span class="fact">{t('mount-latency', { value: milliseconds(status.latencyMs) })}</span>
    {/if}
    <span class="fact selectable">{mount.account}@{mount.lan.host}:{mount.remotePath}</span>
  </div>

  {#if status.state === 'stale'}
    <p class="hint-bar">{t('mount-stale-hint')}</p>
  {/if}
  {#if status.log && (status.state === 'failed' || status.state === 'connecting')}
    <details>
      <summary class="subtle">{t('mount-log')}</summary>
      <pre class="command">{status.log}</pre>
    </details>
  {/if}
  {#if failure}
    <p class="failure" role="alert">
      {t(`error-${failure.key}`)}
      {#if failure.detail}<span class="subtle block">{failure.detail}</span>{/if}
    </p>
  {/if}

  <div class="actions">
    {#if active}
      <button class="button" disabled={busy} onclick={() => run(() => stopMount(mount.id))}>
        {t('mount-unmount')}
      </button>
    {:else}
      <button
        class="button primary"
        disabled={busy}
        onclick={() => run(() => startMount(mount.id))}
      >
        {busy ? t('mount-busy') : t('mount-mount')}
      </button>
    {/if}
    <button
      class="button"
      disabled={status.state !== 'mounted'}
      onclick={() => run(() => openFolder(mount.id))}
    >
      <FolderOpen size={16} aria-hidden="true" />
      {t('mount-open')}
    </button>
    <button
      class="button"
      disabled={status.state !== 'mounted'}
      onclick={() => onaction('selftest')}
    >
      {t('mount-selftest')}
    </button>
    <div class="menu-anchor">
      <button
        class="button"
        aria-haspopup="menu"
        aria-expanded={menu}
        title={t('mount-more')}
        aria-label={t('mount-more')}
        onclick={() => (menu = !menu)}
      >
        <Ellipsis size={16} aria-hidden="true" />
      </button>
      {#if menu}
        <div class="menu" role="menu">
          {#each menuItems as item (item.action)}
            <button
              role="menuitem"
              class:danger={item.action === 'remove'}
              disabled={(item.action === 'speed' || item.action === 'selftest') &&
                status.state !== 'mounted'}
              onclick={() => choose(item.action)}
            >
              {t(item.label, { nas: mount.nasName })}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</article>

<style>
  .mount {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .icon {
    display: grid;
    flex: none;
    place-items: center;
    width: 42px;
    height: 42px;
    border-radius: 12px;
    background: var(--accent-soft);
    color: var(--accent);
  }

  .title {
    flex: 1;
    min-width: 0;
  }

  .title h2 {
    font-size: 1.05em;
  }

  .title p {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .facts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 16px;
  }

  .fact {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }

  details summary {
    cursor: pointer;
  }

  details pre {
    margin-top: 6px;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .menu-anchor {
    position: relative;
  }

  .menu {
    position: absolute;
    right: 0;
    bottom: calc(100% + 4px);
    z-index: 5;
    display: flex;
    flex-direction: column;
    min-width: 220px;
    padding: 4px;
    border-radius: var(--radius-control);
    background: var(--card);
    box-shadow:
      0 0 0 1px var(--card-border),
      0 10px 30px rgb(0 0 0 / 0.2);
  }

  .menu button {
    padding: 7px 10px;
    border: none;
    border-radius: var(--radius-control);
    background: transparent;
    text-align: left;
    cursor: pointer;
  }

  .menu button:hover:not(:disabled) {
    background: var(--hover);
  }

  .menu button:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .menu button.danger {
    color: var(--danger);
  }
</style>
