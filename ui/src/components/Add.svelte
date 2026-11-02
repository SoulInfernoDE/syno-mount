<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import KeyRound from '@lucide/svelte/icons/key-round';
  import ShieldAlert from '@lucide/svelte/icons/shield-alert';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import DsmSignIn from './DsmSignIn.svelte';
  import NasPicker from './NasPicker.svelte';
  import SelfTest from './SelfTest.svelte';
  import {
    cancelSetup,
    connect,
    connectSshOnly,
    createMounts,
    enableServices,
    hasCore,
    installKey,
    isWindows,
    listMounts,
    moreShares,
    parseAddress,
    scanHostKey,
    selfTest,
    setupServices,
    toFailure,
    type Check,
    type Failure,
    type HostKeyView,
    type Installation,
    type KnownDevice,
    type Mount,
    type ServicesView,
  } from '../lib/backend';
  import { t } from '../lib/i18n.svelte';
  import type { NasTarget } from '../lib/nav';

  let { fromMount, ondone }: { fromMount?: string; ondone: () => void } = $props();

  type Step = 'nas' | 'signin' | 'services' | 'key' | 'folders' | 'done';
  const steps: Step[] = ['nas', 'signin', 'services', 'key', 'folders', 'done'];
  const stepLabels: Record<Step, string> = {
    nas: 'add-step-nas',
    signin: 'add-step-sign-in',
    services: 'add-step-services',
    key: 'add-step-key',
    folders: 'add-step-folders',
    done: 'add-step-done',
  };

  let step = $state<Step>('nas');
  let busy = $state(false);
  let failure = $state<Failure | null>(null);

  async function run<T>(job: () => Promise<T>): Promise<T | undefined> {
    busy = true;
    failure = null;
    try {
      return await job();
    } catch (error) {
      failure = toFailure(error);
      return undefined;
    } finally {
      busy = false;
    }
  }

  // Step 1: the NAS.
  type Target = NasTarget;
  let target = $state<Target | null>(null);
  let device = $state<KnownDevice | null>(null);

  function choose(next: Target) {
    target = next;
    account = next.account ?? account;
    failure = null;
    step = 'signin';
  }

  // Step 2: sign in to DSM, or go by SSH alone.
  let account = $state('');
  let password = $state('');
  let sshOnly = $state(false);
  let notAllowed = $state(false);
  let sshPort = $state(22);
  let nasName = $state('');

  async function signedIn(next: KnownDevice) {
    device = next;
    await loadServices();
    step = 'services';
  }

  async function signInSshOnly(event: SubmitEvent) {
    event.preventDefault();
    if (!target) return;
    const current = target;
    const result = await run(() =>
      connectSshOnly({
        host: current.host,
        dsmPort: current.port,
        sshPort,
        name: nasName || current.name,
        account,
        password,
      }),
    );
    if (!result) return;
    device = result;
    step = 'key';
    await readHostKey();
  }

  // Step 3: services.
  let services = $state<ServicesView | null>(null);

  async function loadServices() {
    services = (await run(() => setupServices())) ?? null;
    if (services) sshPort = services.sshPort;
  }

  const changes = $derived(
    services?.services
      ? [
          ...(services.services.sftp ? [] : [t('services-sftp')]),
          ...(services.services.home ? [] : [t('services-home')]),
        ]
      : [],
  );

  async function enable() {
    const next = await run(() => enableServices());
    if (next) {
      services = next;
      sshPort = next.sshPort;
    }
  }

  async function toKey() {
    step = 'key';
    await readHostKey();
  }

  // Step 4: host key and key installation.
  let hostKey = $state<HostKeyView | null>(null);
  let sshPassword = $state('');

  async function readHostKey() {
    hostKey = null;
    hostKey = (await run(() => scanHostKey(sshPort))) ?? null;
  }

  let installation = $state<Installation | null>(null);

  async function install() {
    if (!hostKey) return;
    const fingerprint = hostKey.key.fingerprint;
    const result = await run(() => installKey(fingerprint, sshPassword || undefined));
    if (result) {
      password = '';
      sshPassword = '';
      showFolders(result);
    }
  }

  // Step 5: folders.
  let selected = $state<Record<string, boolean>>({});
  let folders = $state<Record<string, string>>({});
  let atLogin = $state(true);
  let bookmark = $state(true);
  let internet = $state('');
  const internetAddress = $derived(internet.trim() ? parseAddress(internet, 22) : undefined);
  const chosen = $derived(
    installation?.shares.filter((share) => selected[share.name] && !share.mounted) ?? [],
  );

  function showFolders(result: Installation) {
    installation = result;
    selected = Object.fromEntries(result.shares.map((share) => [share.name, false]));
    folders = Object.fromEntries(result.shares.map((share) => [share.name, share.defaultFolder]));
    step = 'folders';
  }

  let created = $state<Mount[]>([]);

  async function create() {
    if (internetAddress === null) return;
    const result = await run(() =>
      createMounts({
        shares: chosen.map((share) => ({
          name: share.name,
          localPath: folders[share.name] ?? share.defaultFolder,
        })),
        atLogin,
        bookmark,
        internet: internetAddress,
      }),
    );
    if (result) {
      created = result;
      step = 'done';
      void testAll();
    }
  }

  // Step 6: a self-test of each new mount once it is mounted.
  let results = $state<Record<string, Check[] | 'waiting' | Failure>>({});
  let stopped = false;

  async function testAll() {
    for (const mount of created) {
      results[mount.id] = 'waiting';
      for (let attempt = 0; attempt < 30 && !stopped; attempt += 1) {
        const views = await listMounts().catch(() => []);
        if (views.find((view) => view.mount.id === mount.id)?.status.state === 'mounted') break;
        await new Promise((resolve) => setTimeout(resolve, 1000));
      }
      try {
        results[mount.id] = await selfTest(mount.id);
      } catch (error) {
        results[mount.id] = toFailure(error);
      }
    }
  }

  async function cancel() {
    await cancelSetup().catch(() => undefined);
    ondone();
  }

  onMount(async () => {
    if (!hasCore()) return;
    if (fromMount) {
      const result = await run(() => moreShares(fromMount));
      if (result) showFolders(result);
      return;
    }
  });

  onDestroy(() => {
    stopped = true;
  });

  const title = $derived(device?.name ?? target?.name ?? target?.host ?? '');
</script>

{#snippet problem()}
  {#if failure}
    <p class="failure" role="alert">
      {t(`error-${failure.key}`)}
      {#if failure.detail}<span class="subtle block">{failure.detail}</span>{/if}
    </p>
  {/if}
{/snippet}

<div class="page">
  <h1>{t('add-title')}</h1>

  <ol class="steps" aria-label={t('add-title')}>
    {#each steps as entry, index (entry)}
      <li
        class:current={entry === step}
        class:past={steps.indexOf(step) > index}
        aria-current={entry === step ? 'step' : undefined}
      >
        <span class="number">{index + 1}</span>
        <span class="label">{t(stepLabels[entry])}</span>
      </li>
    {/each}
  </ol>

  {#if step === 'nas'}
    <NasPicker onchoose={choose} />
    {@render problem()}
    <div class="actions"><button class="button" onclick={cancel}>{t('add-cancel')}</button></div>
  {:else if step === 'signin' && target && sshOnly}
    <section class="card form" aria-busy={busy}>
      <header class="head">
        <div class="icon"><KeyRound size={22} aria-hidden="true" /></div>
        <div>
          <h2>{t('signin-title', { name: title })}</h2>
          <p class="subtle">
            {t('signin-address', { host: target.host, port: String(target.port) })}
          </p>
        </div>
      </header>
      {#if notAllowed}<p class="hint-bar">{t('signin-not-allowed')}</p>{/if}
      <p class="muted">{t('signin-ssh-only-text')}</p>
      <form onsubmit={signInSshOnly}>
        <label class="field">
          <span>{t('signin-name')}</span>
          <input class="input" bind:value={nasName} placeholder={target.name ?? target.host} />
        </label>
        <label class="field">
          <span>{t('signin-ssh-port')}</span>
          <input class="input narrow" type="number" min="1" max="65535" bind:value={sshPort} />
        </label>
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
        {@render problem()}
        <div class="actions">
          <button type="button" class="button" onclick={() => (sshOnly = false)}
            >{t('add-back')}</button
          >
          <button class="button primary" disabled={busy}
            >{busy ? t('signin-busy') : t('signin-continue')}</button
          >
        </div>
      </form>
    </section>
  {:else if step === 'signin' && target}
    <DsmSignIn
      {target}
      text={t('signin-text')}
      signIn={connect}
      bind:account
      bind:password
      onconnected={signedIn}
      onnotallowed={() => {
        notAllowed = true;
        sshOnly = true;
      }}
      onback={() => (step = 'nas')}
    >
      {#snippet extra()}
        <button type="button" class="button" onclick={() => (sshOnly = true)}>
          {t('signin-ssh-only')}
        </button>
      {/snippet}
    </DsmSignIn>
  {:else if step === 'services'}
    <section class="card form" aria-busy={busy}>
      <h2>{t('services-title', { name: title })}</h2>
      <p class="muted">{t('services-text')}</p>
      {#if busy && !services}
        <p class="muted">{t('services-checking')}</p>
      {:else if services?.services}
        <ul class="list">
          <li>
            <span class="label">{t('services-sftp')}</span>
            <span class="badge {services.services.sftp ? 'success' : 'danger'}">
              {t(services.services.sftp ? 'services-on' : 'services-off')}
            </span>
          </li>
          <li>
            <span class="label">{t('services-home')}</span>
            <span class="badge {services.services.home ? 'success' : 'danger'}">
              {t(services.services.home ? 'services-on' : 'services-off')}
            </span>
          </li>
        </ul>
        <p class="subtle">{t('services-port', { port: String(services.services.sftpPort) })}</p>
        {#if changes.length > 0}
          <p class="hint-bar">{t('services-enable-text', { changes: changes.join(', ') })}</p>
        {/if}
      {:else}
        <p class="hint-bar">{t('services-unknown')}</p>
        <label class="field">
          <span>{t('signin-ssh-port')}</span>
          <input class="input narrow" type="number" min="1" max="65535" bind:value={sshPort} />
        </label>
      {/if}
      {@render problem()}
      <div class="actions">
        <button class="button" onclick={cancel}>{t('add-cancel')}</button>
        {#if changes.length > 0}
          <button class="button primary" disabled={busy} onclick={enable}
            >{t('services-enable')}</button
          >
        {:else}
          <button class="button primary" disabled={busy} onclick={toKey}>{t('add-next')}</button>
        {/if}
      </div>
    </section>
  {:else if step === 'key'}
    <section class="card form" aria-busy={busy}>
      <header class="head">
        <div class="icon" class:warning={hostKey?.changed}>
          {#if hostKey?.changed}<ShieldAlert size={22} aria-hidden="true" />{:else}<ShieldCheck
              size={22}
              aria-hidden="true"
            />{/if}
        </div>
        <h2>{t('key-title', { name: title })}</h2>
      </header>
      {#if !hostKey && busy}
        <p class="muted">{t('key-checking')}</p>
      {:else if hostKey}
        <p class="muted">{t('key-text')}</p>
        <div class="fingerprint">
          <span class="subtle">{t('key-fingerprint', { algorithm: hostKey.key.algorithm })}</span>
          <code>{hostKey.key.fingerprint}</code>
        </div>
        {#if hostKey.known}<p class="subtle">{t('key-known')}</p>{/if}
        {#if hostKey.changed}<p class="failure">{t('key-changed')}</p>{/if}
        {#if !sshOnly}
          <label class="field">
            <span>{t('key-password')}</span>
            <input
              class="input"
              type="password"
              bind:value={sshPassword}
              autocomplete="current-password"
            />
            <span class="subtle">{t('key-password-hint')}</span>
          </label>
        {/if}
        <p class="subtle">{t('key-explain')}</p>
      {/if}
      {@render problem()}
      <div class="actions">
        <button class="button" onclick={cancel}>{t('add-cancel')}</button>
        {#if !hostKey}
          <button class="button" disabled={busy} onclick={readHostKey}>{t('system-again')}</button>
        {:else}
          <button
            class="button {hostKey.changed ? 'danger' : 'primary'}"
            disabled={busy}
            onclick={install}
          >
            {busy ? t('key-installing') : t('key-install')}
          </button>
        {/if}
      </div>
    </section>
  {:else if step === 'folders'}
    <section class="card form" aria-busy={busy}>
      <h2>{t('folders-title')}</h2>
      <p class="muted">{t('folders-text')}</p>
      {#if installation?.chrooted}<p class="subtle">{t('folders-chrooted')}</p>{/if}
      {#if installation && installation.shares.length === 0}
        <p class="hint-bar">{t('folders-none')}</p>
      {/if}
      <ul class="list shares">
        {#each installation?.shares ?? [] as share (share.name)}
          <li>
            <label class="check-row share">
              <input type="checkbox" bind:checked={selected[share.name]} disabled={share.mounted} />
              <span>
                <strong>{share.name}</strong>
                <span class="subtle block">{share.remotePath}</span>
              </span>
            </label>
            {#if share.mounted}
              <span class="badge">{t('folders-mounted')}</span>
            {:else if selected[share.name]}
              <input
                class="input local"
                bind:value={folders[share.name]}
                aria-label={t(isWindows() ? 'folders-local-windows' : 'folders-local')}
                spellcheck="false"
              />
            {/if}
          </li>
        {/each}
      </ul>
      <label class="check-row">
        <input type="checkbox" bind:checked={atLogin} />
        {t('folders-at-login')}
      </label>
      {#if !isWindows()}
        <label class="check-row">
          <input type="checkbox" bind:checked={bookmark} />
          {t('folders-bookmark')}
        </label>
      {/if}
      {#if !fromMount}
        <label class="field">
          <span>{t('folders-internet')}</span>
          <input
            class="input"
            bind:value={internet}
            placeholder={t('settings-internet-placeholder')}
            spellcheck="false"
            aria-invalid={internetAddress === null}
          />
          <span class="subtle">{t('settings-internet-hint')}</span>
          {#if internetAddress === null}<span class="invalid">{t('settings-internet-invalid')}</span
            >{/if}
        </label>
      {/if}
      {@render problem()}
      <div class="actions">
        <button class="button" onclick={cancel}>{t('add-cancel')}</button>
        <button
          class="button primary"
          disabled={busy || chosen.length === 0 || internetAddress === null}
          onclick={create}
        >
          {busy ? t('folders-creating') : t('folders-create', { count: chosen.length })}
        </button>
      </div>
    </section>
  {:else if step === 'done'}
    <section class="card form">
      <h2>{t('done-title')}</h2>
      <p class="muted">{t('done-text', { count: created.length })}</p>
      {#each created as mount (mount.id)}
        {@const result = results[mount.id]}
        <div class="result">
          <h3>{t('selftest-title', { share: mount.share })}</h3>
          {#if result === undefined || result === 'waiting'}
            <p class="muted">{t('selftest-running')}</p>
          {:else if Array.isArray(result)}
            <SelfTest checks={result} />
          {:else}
            <p class="failure">
              {t(`error-${result.key}`)} <span class="subtle block">{result.detail}</span>
            </p>
          {/if}
        </div>
      {/each}
      <div class="actions">
        <button class="button primary" onclick={ondone}>{t('done-finish')}</button>
      </div>
    </section>
  {/if}
</div>

<style>
  .steps {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 14px;
    margin: 0;
    padding: 0;
    list-style: none;
    color: var(--text-3);
  }

  .steps li {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .number {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--track);
    font-size: 0.82em;
    font-weight: 600;
  }

  .steps .current {
    color: var(--text);
    font-weight: 600;
  }

  .steps .current .number {
    background: var(--accent);
    color: var(--accent-text);
  }

  .steps .past .number {
    background: var(--accent-soft);
    color: var(--accent);
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

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

  .label {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    color: var(--text);
    line-height: 1.3;
  }

  .shares .share {
    flex: 1;
    color: var(--text);
  }

  .local {
    flex: 1.4;
    min-width: 0;
  }

  .narrow {
    max-width: 8em;
  }

  .invalid {
    color: var(--danger);
    font-size: 0.92em;
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

  .result {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-top: 6px;
    border-top: 1px solid var(--card-border);
  }

  @media (max-width: 560px) {
    .steps .label {
      display: none;
    }

    .list li {
      flex-wrap: wrap;
    }
  }
</style>
