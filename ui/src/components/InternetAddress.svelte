<script lang="ts">
  import {
    checkAddress,
    isWindows,
    parseAddress,
    toFailure,
    type Address,
    type AddressCheck,
    type Failure,
  } from '../lib/backend';
  import { tick } from 'svelte';
  import { t } from '../lib/i18n.svelte';

  let {
    initial,
    address = $bindable(),
    mountId,
    label,
  }: {
    initial?: Address;
    /** The address as typed: `undefined` when empty, `null` when not valid. */
    address?: Address | null;
    /** With a mount, the address can be checked against its pinned host key. */
    mountId?: string;
    label: string;
  } = $props();

  // The form starts from the saved address once; later edits belong to the user.
  // svelte-ignore state_referenced_locally
  const start = initial;
  let text = $state(start ? `${start.host}:${start.port}` : '');
  let https = $state(Boolean(start?.tunnel));
  let serverName = $state(
    start?.tunnel && start.tunnel.serverName !== start.host ? start.tunnel.serverName : '',
  );
  let alpn = $state(start?.tunnel?.alpn ?? true);
  let pinned = $state(start?.tunnel?.pinnedCertificate);

  let checking = $state(false);
  let result = $state<AddressCheck | null>(null);
  let failure = $state<Failure | null>(null);

  const parsed = $derived(text.trim() ? parseAddress(text, https ? 443 : 22) : undefined);

  $effect(() => {
    if (!parsed) {
      address = parsed;
      return;
    }
    address = https
      ? {
          ...parsed,
          tunnel: {
            serverName: serverName.trim() || parsed.host,
            alpn,
            ...(pinned ? { pinnedCertificate: pinned } : {}),
          },
        }
      : { ...parsed };
  });

  // A changed address needs a new check.
  $effect(() => {
    void address;
    result = null;
    failure = null;
  });

  async function check() {
    if (!mountId || !address) return;
    checking = true;
    result = null;
    failure = null;
    try {
      result = await checkAddress(mountId, address);
    } catch (error) {
      failure = toFailure(error);
    } finally {
      checking = false;
    }
  }

  /** Trusts the self-signed certificate the proxy showed, then checks again. */
  async function trust() {
    pinned = failure?.detail;
    // Let the address pick up the pinned certificate first.
    await tick();
    await check();
  }
</script>

<div class="internet">
  <label class="field">
    <span>{label}</span>
    <input
      class="input"
      bind:value={text}
      placeholder={https ? t('internet-https-placeholder') : t('settings-internet-placeholder')}
      spellcheck="false"
      aria-invalid={parsed === null}
    />
    <span class="subtle">{https ? t('internet-https-hint') : t('settings-internet-hint')}</span>
    {#if parsed === null}<span class="invalid">{t('settings-internet-invalid')}</span>{/if}
  </label>

  {#if !isWindows()}
    <label class="check-row">
      <input type="checkbox" bind:checked={https} />
      <span>{t('internet-https')}<span class="subtle block">{t('internet-https-text')}</span></span>
    </label>
  {/if}

  {#if https}
    <details class="advanced">
      <summary>{t('internet-https-advanced')}</summary>
      <label class="field">
        <span>{t('internet-server-name')}</span>
        <input
          class="input"
          bind:value={serverName}
          placeholder={parsed?.host ?? 'ssh.example.org'}
          spellcheck="false"
        />
        <span class="subtle">{t('internet-server-name-hint')}</span>
      </label>
      <label class="check-row">
        <input type="checkbox" bind:checked={alpn} />
        <span>{t('internet-alpn')}<span class="subtle block">{t('internet-alpn-hint')}</span></span>
      </label>
      {#if pinned}
        <div class="fingerprint">
          <span class="subtle">{t('internet-pinned')}</span>
          <code>{pinned}</code>
          <button class="button small" type="button" onclick={() => (pinned = undefined)}
            >{t('internet-pinned-forget')}</button
          >
        </div>
      {/if}
    </details>
  {/if}

  {#if mountId && address}
    <div class="check">
      <button class="button small" type="button" disabled={checking} onclick={check}>
        {checking ? t('internet-checking') : t('internet-check')}
      </button>
      {#if result}
        <span class="ok" role="status"
          >{t('internet-check-ok', { ms: Math.round(result.latencyMs) })}</span
        >
      {/if}
    </div>
    {#if failure}
      <div class="failure" role="alert">
        {t(`error-${failure.key}`)}
        {#if failure.key === 'proxy-certificate-untrusted' && failure.detail}
          <span class="block subtle">{t('internet-cert-text')}</span>
          <div class="fingerprint">
            <span class="subtle">{t('signin-cert-fingerprint')}</span>
            <code>{failure.detail}</code>
          </div>
          <button class="button small" type="button" onclick={trust}
            >{t('internet-cert-trust')}</button
          >
        {:else if failure.detail}
          <span class="subtle block">{failure.detail}</span>
        {/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .internet {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .advanced {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .advanced summary {
    cursor: pointer;
    color: var(--text-2);
  }

  .advanced[open] summary {
    margin-bottom: 10px;
  }

  .advanced[open] > :not(summary) + :not(summary) {
    margin-top: 12px;
  }

  .fingerprint {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 10px 12px;
    margin: 8px 0;
    border-radius: var(--radius-control);
    background: var(--track);
  }

  .fingerprint code {
    word-break: break-all;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }

  .ok {
    color: var(--success);
  }

  .invalid {
    color: var(--danger);
    font-size: 0.92em;
  }
</style>
