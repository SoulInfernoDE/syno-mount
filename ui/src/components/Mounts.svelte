<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import FolderPlus from '@lucide/svelte/icons/folder-plus';
  import Info from '@lucide/svelte/icons/info';
  import icon from '../../../assets/branding/icon.svg';
  import MountCard from './MountCard.svelte';
  import MountDialogs from './MountDialogs.svelte';
  import { hasCore, listMounts, systemCheck, type MountView } from '../lib/backend';
  import { t } from '../lib/i18n.svelte';
  import type { MountAction } from '../lib/nav';

  let {
    onadd,
    onsystem,
  }: {
    /** Starts the assistant; with a mount ID it offers more folders of that NAS. */
    onadd: (fromMount?: string) => void;
    onsystem: () => void;
  } = $props();

  let views = $state<MountView[]>([]);
  let loaded = $state(false);
  let systemReady = $state(true);
  let dialog = $state<{ id: string; action: MountAction | null } | null>(null);

  const dialogMount = $derived(views.find((view) => view.mount.id === dialog?.id)?.mount);

  async function refresh() {
    if (!hasCore()) return;
    try {
      views = await listMounts();
    } finally {
      loaded = true;
    }
  }

  let timer: ReturnType<typeof setInterval> | undefined;
  onMount(async () => {
    if (!hasCore()) return;
    void systemCheck().then((check) => {
      systemReady = check.sshfs !== null && check.fusermount && check.fuseDevice;
    });
    await refresh();
    timer = setInterval(refresh, 5000);
  });
  onDestroy(() => clearInterval(timer));

  function act(id: string, action: MountAction) {
    if (action === 'more') {
      onadd(id);
      return;
    }
    dialog = { id, action };
  }

  $effect(() => {
    if (dialog && dialog.action === null) dialog = null;
  });
</script>

<div class="page">
  <header class="top">
    <div>
      <h1>{t('mounts-title')}</h1>
      <p class="muted">{t('mounts-subtitle')}</p>
    </div>
    {#if views.length > 0}
      <button class="button primary" onclick={() => onadd()}>
        <FolderPlus size={16} aria-hidden="true" />
        {t('mounts-add')}
      </button>
    {/if}
  </header>

  {#if !systemReady}
    <p class="hint-bar row">
      <span>{t('mounts-system-problem')}</span>
      <button class="button small" onclick={onsystem}>{t('mounts-system-open')}</button>
    </p>
  {/if}

  {#if !hasCore()}
    <p class="card notice muted">
      <Info size={16} aria-hidden="true" />
      {t('mounts-preview')}
    </p>
  {:else if loaded && views.length === 0}
    <section class="card empty">
      <img src={icon} alt="" width="88" height="88" />
      <h2>{t('mounts-empty-title')}</h2>
      <p class="muted">{t('mounts-empty-text')}</p>
      <button class="button primary" onclick={() => onadd()}>
        <FolderPlus size={16} aria-hidden="true" />
        {t('mounts-add')}
      </button>
    </section>
  {/if}

  {#each views as view (view.mount.id)}
    <MountCard {view} onchanged={refresh} onaction={(action) => act(view.mount.id, action)} />
  {/each}

  <footer class="subtle">{t('unofficial')}</footer>
</div>

{#if dialog && dialogMount}
  {#key dialog.id + dialog.action}
    <MountDialogs mount={dialogMount} bind:action={dialog.action} onchanged={refresh} />
  {/key}
{/if}

<style>
  .top {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }

  .top p {
    margin-top: 4px;
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .notice {
    display: flex;
    gap: 8px;
    align-items: flex-start;
  }

  .notice :global(svg) {
    flex: none;
    margin-top: 2px;
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 36px 24px;
    text-align: center;
  }

  .empty img {
    filter: drop-shadow(0 6px 16px rgb(0 0 0 / 0.18));
  }

  .empty p {
    max-width: 480px;
  }

  footer {
    margin-top: 12px;
    text-align: center;
  }

  @media (max-width: 560px) {
    .top {
      flex-direction: column;
    }
  }
</style>
