<script lang="ts">
  import { onMount } from 'svelte';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import { hasCore, systemCheck, type SystemCheck } from '../lib/backend';
  import { t } from '../lib/i18n.svelte';

  let check = $state<SystemCheck | null>(null);
  let busy = $state(false);

  async function load() {
    if (!hasCore()) return;
    busy = true;
    try {
      check = await systemCheck();
    } finally {
      busy = false;
    }
  }

  onMount(load);

  const rows = $derived(
    check
      ? [
          {
            label: t('system-sshfs'),
            ok: check.sshfs !== null,
            warn: check.sshfs !== null && !check.sshfsParallel,
            text: check.sshfs
              ? t(check.sshfsParallel ? 'system-sshfs-version' : 'system-sshfs-old', {
                  version: check.sshfs,
                })
              : t('system-missing'),
          },
          {
            label: t('system-fusermount'),
            ok: check.fusermount,
            warn: false,
            text: t(check.fusermount ? 'system-ok' : 'system-missing'),
          },
          {
            label: t('system-fuse-device'),
            ok: check.fuseDevice,
            warn: false,
            text: t(check.fuseDevice ? 'system-ok' : 'system-missing'),
          },
          {
            label: t('system-systemd'),
            ok: check.systemdUser,
            warn: false,
            text: t(check.systemdUser ? 'system-ok' : 'system-systemd-missing'),
          },
        ]
      : [],
  );
</script>

<div class="page">
  <h1>{t('system-title')}</h1>
  <p class="muted">{t('system-text')}</p>

  <section class="card">
    <ul>
      {#each rows as row (row.label)}
        <li>
          <span class="state" class:ok={row.ok && !row.warn} class:warn={row.warn}>
            {#if row.warn}
              <TriangleAlert size={18} aria-hidden="true" />
            {:else if row.ok}
              <CircleCheck size={18} aria-hidden="true" />
            {:else}
              <CircleX size={18} aria-hidden="true" />
            {/if}
          </span>
          <span class="label">{row.label}</span>
          <span class="muted">{row.text}</span>
        </li>
      {/each}
    </ul>
    {#if check?.installHint}
      <p>{t('system-install')}</p>
      <pre class="command">{check.installHint}</pre>
    {/if}
    <div class="actions">
      <button class="button" onclick={load} disabled={busy}>{t('system-again')}</button>
    </div>
  </section>
</div>

<style>
  ul {
    margin: 0 0 8px;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 2px;
  }

  li + li {
    border-top: 1px solid var(--card-border);
  }

  .label {
    flex: 1;
  }

  .state {
    display: flex;
    color: var(--danger);
  }

  .state.ok {
    color: var(--success);
  }

  .state.warn {
    color: var(--warning);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 10px;
  }
</style>
