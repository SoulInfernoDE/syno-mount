<script lang="ts">
  import { onMount } from 'svelte';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Server from '@lucide/svelte/icons/server';
  import Trash from '@lucide/svelte/icons/trash-2';
  import {
    discover,
    forgetDevice,
    hasCore,
    knownDevices,
    parseAddress,
    toFailure,
    type Failure,
    type FoundNas,
    type KnownDevice,
  } from '../lib/backend';
  import { t } from '../lib/i18n.svelte';
  import type { NasTarget } from '../lib/nav';

  let { onchoose }: { onchoose: (target: NasTarget) => void } = $props();

  let saved = $state<KnownDevice[]>([]);
  let found = $state<FoundNas[]>([]);
  let searching = $state(false);
  let failure = $state<Failure | null>(null);
  let address = $state('');
  let addressInvalid = $state(false);

  const unsaved = $derived(
    found.filter(
      (nas) => !saved.some((known) => known.id === `${nas.host.toLowerCase()}:${nas.port}`),
    ),
  );

  async function search() {
    searching = true;
    failure = null;
    try {
      found = await discover();
    } catch (error) {
      failure = toFailure(error);
    } finally {
      searching = false;
    }
  }

  function chooseAddress(event: SubmitEvent) {
    event.preventDefault();
    const parsed = parseAddress(address);
    addressInvalid = parsed === null;
    if (parsed) onchoose(parsed);
  }

  async function forget(id: string) {
    await forgetDevice(id).catch(() => undefined);
    saved = await knownDevices();
  }

  onMount(async () => {
    if (!hasCore()) return;
    saved = await knownDevices().catch(() => []);
    await search();
  });
</script>

{#if saved.length > 0}
  <section class="card">
    <h2>{t('add-saved')}</h2>
    <ul class="list">
      {#each saved as known (known.id)}
        <li>
          <Server size={22} strokeWidth={1.6} aria-hidden="true" />
          <div class="device">
            <strong>{known.name}</strong>
            <span class="subtle"
              >{[known.model, `${known.host}:${known.port}`, known.account]
                .filter(Boolean)
                .join(' · ')}</span
            >
          </div>
          <button
            class="icon-button"
            title={t('add-forget')}
            aria-label={t('add-forget')}
            onclick={() => forget(known.id)}
          >
            <Trash size={16} aria-hidden="true" />
          </button>
          <button class="button primary" onclick={() => onchoose(known)}>{t('add-choose')}</button>
        </li>
      {/each}
    </ul>
  </section>
{/if}

<section class="card">
  <div class="title-row">
    <h2>{t('add-found')}</h2>
    <button class="button small" onclick={search} disabled={searching}>
      <span class:spin={searching}><RefreshCw size={14} aria-hidden="true" /></span>
      {searching ? t('add-searching') : t('add-search-again')}
    </button>
  </div>
  {#if unsaved.length > 0}
    <ul class="list">
      {#each unsaved as nas (nas.host)}
        <li>
          <Server size={22} strokeWidth={1.6} aria-hidden="true" />
          <div class="device">
            <strong>{nas.name}</strong>
            <span class="subtle"
              >{[nas.model, nas.host, nas.dsm ? `DSM ${nas.dsm}` : null]
                .filter(Boolean)
                .join(' · ')}</span
            >
          </div>
          <button class="button" onclick={() => onchoose(nas)}>{t('add-choose')}</button>
        </li>
      {/each}
    </ul>
  {:else if failure}
    <p class="subtle">{t(`error-${failure.key}`)}</p>
  {:else if !searching && found.length === 0}
    <p class="subtle">{t('add-none-found')}</p>
  {/if}

  <form class="manual" onsubmit={chooseAddress}>
    <label for="address">{t('add-manual')}</label>
    <div class="row">
      <input
        id="address"
        class="input"
        bind:value={address}
        placeholder={t('add-manual-placeholder')}
        autocomplete="off"
        spellcheck="false"
        aria-invalid={addressInvalid}
      />
      <button class="button" disabled={!address.trim()}>{t('add-choose')}</button>
    </div>
    {#if addressInvalid}<span class="invalid">{t('add-manual-invalid')}</span>{/if}
  </form>
</section>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .list {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .list li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 4px;
    color: var(--text-2);
  }

  .list li + li {
    border-top: 1px solid var(--card-border);
  }

  .device {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    color: var(--text);
    line-height: 1.3;
  }

  .icon-button {
    display: grid;
    place-items: center;
    width: var(--control-height);
    height: var(--control-height);
    border: none;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
  }

  .icon-button:hover {
    background: var(--hover);
    color: var(--danger);
  }

  .manual {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .row {
    display: flex;
    gap: 8px;
  }

  .row input {
    flex: 1;
    min-width: 0;
  }

  .invalid {
    color: var(--danger);
    font-size: 0.92em;
  }

  @media (max-width: 560px) {
    .list li {
      flex-wrap: wrap;
    }
  }
</style>
