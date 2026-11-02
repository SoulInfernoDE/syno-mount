<script lang="ts">
  import type { Snippet } from 'svelte';
  import KeyRound from '@lucide/svelte/icons/key-round';
  import ShieldAlert from '@lucide/svelte/icons/shield-alert';
  import {
    toFailure,
    type ConnectOutcome,
    type ConnectRequest,
    type Failure,
    type KnownDevice,
  } from '../lib/backend';
  import { t } from '../lib/i18n.svelte';
  import type { NasTarget } from '../lib/nav';

  let {
    target,
    text,
    signIn,
    account = $bindable(''),
    password = $bindable(''),
    onconnected,
    onnotallowed,
    onback,
    extra,
  }: {
    target: NasTarget;
    /** What the sign-in is for, shown above the form. */
    text: string;
    signIn: (request: ConnectRequest) => Promise<ConnectOutcome>;
    account?: string;
    password?: string;
    onconnected: (device: KnownDevice) => void;
    /** DSM does not let this account in (error 402). */
    onnotallowed?: () => void;
    onback: () => void;
    /** More buttons on the left of the form's actions. */
    extra?: Snippet;
  } = $props();

  let otp = $state('');
  let needOtp = $state(false);
  let certificate = $state<{ fingerprint: string; changed: boolean } | null>(null);
  let trustFingerprint = $state<string | undefined>();
  let busy = $state(false);
  let failure = $state<Failure | null>(null);

  async function submit(event?: SubmitEvent) {
    event?.preventDefault();
    busy = true;
    failure = null;
    try {
      const outcome = await signIn({
        host: target.host,
        port: target.port,
        name: target.name,
        model: target.model,
        account,
        password,
        otp: needOtp ? otp : undefined,
        trustFingerprint,
      });
      switch (outcome.status) {
        case 'connected':
          onconnected(outcome.device);
          break;
        case 'otpRequired':
          needOtp = true;
          break;
        case 'untrustedCertificate':
        case 'certificateChanged':
          certificate = {
            fingerprint: outcome.fingerprint,
            changed: outcome.status === 'certificateChanged',
          };
          break;
        case 'notAllowed':
          if (onnotallowed) onnotallowed();
          else failure = { key: 'account-not-allowed' };
          break;
        case 'failed':
          failure = { key: outcome.key, detail: outcome.detail };
          break;
      }
    } catch (error) {
      failure = toFailure(error);
    } finally {
      busy = false;
    }
  }

  function trust() {
    trustFingerprint = certificate?.fingerprint;
    certificate = null;
    void submit();
  }
</script>

<section class="card form" aria-busy={busy}>
  <header class="head">
    <div class="icon" class:warning={certificate?.changed}>
      {#if certificate?.changed}
        <ShieldAlert size={22} aria-hidden="true" />
      {:else}
        <KeyRound size={22} aria-hidden="true" />
      {/if}
    </div>
    <div>
      <h2>{t('signin-title', { name: target.name ?? target.host })}</h2>
      <p class="subtle">
        {t('signin-address', { host: target.host, port: String(target.port) })}
      </p>
    </div>
  </header>

  {#if certificate}
    <h3>{t(certificate.changed ? 'signin-cert-changed-title' : 'signin-cert-title')}</h3>
    <p class="muted">{t(certificate.changed ? 'signin-cert-changed-text' : 'signin-cert-text')}</p>
    <div class="fingerprint">
      <span class="subtle">{t('signin-cert-fingerprint')}</span>
      <code>{certificate.fingerprint}</code>
    </div>
    <div class="actions">
      <button class="button" onclick={() => (certificate = null)}>{t('add-back')}</button>
      <button class="button {certificate.changed ? 'danger' : 'primary'}" onclick={trust}>
        {t(certificate.changed ? 'signin-cert-trust-changed' : 'signin-cert-trust')}
      </button>
    </div>
  {:else}
    <p class="muted">{text}</p>
    <form onsubmit={submit}>
      {#if needOtp}
        <h3>{t('signin-otp-title')}</h3>
        <p class="muted">{t('signin-otp-text')}</p>
        <label class="field">
          <span>{t('signin-otp-code')}</span>
          <input
            class="input code"
            bind:value={otp}
            inputmode="numeric"
            autocomplete="one-time-code"
            maxlength="8"
            required
          />
        </label>
      {:else}
        <label class="field">
          <span>{t('signin-account')}</span>
          <input
            class="input"
            bind:value={account}
            autocomplete="username"
            spellcheck="false"
            required
          />
        </label>
        <label class="field">
          <span>{t('signin-password')}</span>
          <input
            class="input"
            type="password"
            bind:value={password}
            autocomplete="current-password"
            required
          />
        </label>
      {/if}
      {#if failure}
        <p class="failure" role="alert">
          {t(`error-${failure.key}`)}
          {#if failure.detail}<span class="subtle block">{failure.detail}</span>{/if}
        </p>
      {/if}
      <div class="actions spread">
        <span class="actions"
          >{#if extra}{@render extra()}{/if}</span
        >
        <span class="actions">
          <button type="button" class="button" onclick={onback}>{t('add-back')}</button>
          <button class="button primary" disabled={busy}>
            {busy ? t('signin-busy') : needOtp ? t('signin-otp-submit') : t('signin-submit')}
          </button>
        </span>
      </div>
    </form>
  {/if}
</section>

<style>
  .card,
  form {
    display: flex;
    flex-direction: column;
    gap: 14px;
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

  .icon.warning {
    background: color-mix(in srgb, var(--danger) 16%, transparent);
    color: var(--danger);
  }

  h3 {
    font-size: 1em;
  }

  .code {
    max-width: 10em;
    font-size: 1.3em;
    letter-spacing: 0.2em;
    font-variant-numeric: tabular-nums;
  }

  .fingerprint {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    border-radius: var(--radius-control);
    background: var(--track);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
  }

  .actions.spread {
    justify-content: space-between;
  }
</style>
