<script lang="ts">
  import { onDestroy } from 'svelte';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import LogOut from '@lucide/svelte/icons/log-out';
  import Dialog from './Dialog.svelte';
  import DsmSignIn from './DsmSignIn.svelte';
  import NasPicker from './NasPicker.svelte';
  import {
    adminApply,
    adminConnect,
    adminDisconnect,
    adminEnableServices,
    adminOverview,
    adminShareAccess,
    toFailure,
    type Access,
    type Account,
    type AdminChanges,
    type AdminOverview,
    type Failure,
    type Permission,
    type ShareAccess,
  } from '../lib/backend';
  import { t } from '../lib/i18n.svelte';
  import type { NasTarget } from '../lib/nav';

  type Step = 'pick' | 'signin' | 'ready';
  let step = $state<Step>('pick');
  let target = $state<NasTarget | null>(null);
  let overview = $state<AdminOverview | null>(null);
  let share = $state('');
  let access = $state<ShareAccess[]>([]);
  let busy = $state(false);
  let failure = $state<Failure | null>(null);

  /** The SFTP choice per account: 'default', 'allowed' or 'denied'. */
  type SftpChoice = 'default' | Access;
  let sftpEdits = $state<Record<string, SftpChoice>>({});
  /** Edits per shared folder and account. */
  let shareEdits = $state<Record<string, Record<string, Permission>>>({});
  let confirming = $state(false);
  let saved = $state(false);

  async function run<T>(job: () => Promise<T>): Promise<T | undefined> {
    busy = true;
    failure = null;
    try {
      return await job();
    } catch (error) {
      failure = toFailure(error);
      if (failure.key === 'session-expired' || failure.key === 'not-connected') step = 'signin';
      return undefined;
    } finally {
      busy = false;
    }
  }

  async function load() {
    const next = await run(() => adminOverview());
    if (!next) return;
    overview = next;
    if (!next.shares.some((entry) => entry.name === share)) share = next.shares[0]?.name ?? '';
    await loadAccess();
  }

  async function loadAccess() {
    if (!share) {
      access = [];
      return;
    }
    const current = share;
    access = (await run(() => adminShareAccess(current))) ?? [];
  }

  function choose(next: NasTarget) {
    target = next;
    step = 'signin';
  }

  async function signedIn() {
    step = 'ready';
    saved = false;
    await load();
  }

  async function signOut() {
    await adminDisconnect().catch(() => undefined);
    overview = null;
    sftpEdits = {};
    shareEdits = {};
    step = 'pick';
  }

  onDestroy(() => void adminDisconnect().catch(() => undefined));

  function ruleOf(account: Account): SftpChoice {
    return account.sftpRule === 'default' ? 'default' : account.sftpRule.own;
  }

  function sftpChoice(account: Account): SftpChoice {
    return sftpEdits[account.name] ?? ruleOf(account);
  }

  function setSftp(account: Account, choice: SftpChoice) {
    if (choice === ruleOf(account)) delete sftpEdits[account.name];
    else sftpEdits[account.name] = choice;
  }

  /** What an account ends up with: its own rule, or the rule for everyone. */
  function sftpEffective(account: Account): Access {
    const choice = sftpChoice(account);
    if (choice !== 'default') return choice;
    if (account.sftpRule === 'default') return account.sftp;
    // The rule for everyone, as the accounts without their own rule show it.
    return overview?.accounts.find((other) => other.sftpRule === 'default')?.sftp ?? 'allowed';
  }

  function permissionOf(name: string): Permission {
    return access.find((entry) => entry.account === name)?.permission ?? 'none';
  }

  function permissionChoice(name: string): Permission {
    return shareEdits[share]?.[name] ?? permissionOf(name);
  }

  function setPermission(name: string, permission: Permission) {
    const edits = { ...(shareEdits[share] ?? {}) };
    if (permission === permissionOf(name)) delete edits[name];
    else edits[name] = permission;
    shareEdits[share] = edits;
  }

  function isAdmin(name: string): boolean {
    return access.find((entry) => entry.account === name)?.admin ?? false;
  }

  /** Ready for Syno Mount: may sign in, may use SFTP, reaches the selected folder. */
  function ready(account: Account): boolean {
    const permission = permissionChoice(account.name);
    const reaches =
      permission === 'readOnly' ||
      permission === 'readWrite' ||
      permission === 'custom' ||
      (isAdmin(account.name) && permission !== 'deny');
    return !account.disabled && sftpEffective(account) === 'allowed' && reaches;
  }

  const changes = $derived<AdminChanges>({
    sftp: Object.entries(sftpEdits).map(([account, choice]) => ({
      account,
      access: choice === 'default' ? null : choice,
    })),
    shares: Object.entries(shareEdits).flatMap(([folder, edits]) =>
      Object.entries(edits).map(([account, permission]) => ({
        share: folder,
        account,
        permission,
      })),
    ),
  });
  const changeCount = $derived(changes.sftp.length + changes.shares.length);

  function describeSftp(choice: Access | null): string {
    return t(choice === null ? 'admin-sftp-default' : `admin-sftp-${choice}`);
  }

  async function apply() {
    const done = await run(async () => {
      await adminApply(changes);
      return true;
    });
    confirming = false;
    if (done) {
      sftpEdits = {};
      shareEdits = {};
      saved = true;
      await load();
    }
  }

  async function enableServices() {
    const services = await run(() => adminEnableServices());
    if (services && overview) overview.services = services;
  }

  const servicesOff = $derived(
    overview ? !overview.services.sftp || !overview.services.home : false,
  );
  const shareInfo = $derived(overview?.shares.find((entry) => entry.name === share));
  const permissions: Permission[] = ['none', 'deny', 'readOnly', 'readWrite'];
</script>

<div class="page">
  <h1>{t('admin-title')}</h1>
  <p class="muted">{t('admin-subtitle')}</p>

  {#if step === 'pick'}
    <NasPicker onchoose={choose} />
  {:else if step === 'signin' && target}
    <DsmSignIn
      {target}
      text={t('admin-signin-text')}
      signIn={adminConnect}
      account={target.account ?? ''}
      onconnected={signedIn}
      onback={() => (step = 'pick')}
    />
  {:else if overview}
    <section class="card">
      <div class="title-row">
        <h2>{overview.device.name}</h2>
        <button class="button small" onclick={signOut}>
          <LogOut size={14} aria-hidden="true" />
          {t('admin-sign-out')}
        </button>
      </div>
      <ul class="services">
        <li>
          {#if overview.services.sftp}<CircleCheck size={18} class="ok" />{:else}<CircleX
              size={18}
              class="bad"
            />{/if}
          <span>{t('services-sftp')}</span>
        </li>
        <li>
          {#if overview.services.home}<CircleCheck size={18} class="ok" />{:else}<CircleX
              size={18}
              class="bad"
            />{/if}
          <span>{t('services-home')}</span>
        </li>
      </ul>
      {#if servicesOff}
        <div class="actions">
          <button class="button primary" disabled={busy} onclick={enableServices}>
            {t('services-enable')}
          </button>
        </div>
      {/if}
    </section>

    <section class="card">
      <div class="title-row">
        <h2>{t('admin-accounts')}</h2>
        <label class="share-picker">
          <span class="subtle">{t('admin-share')}</span>
          <select class="select" bind:value={share} onchange={loadAccess}>
            {#each overview.shares as entry (entry.name)}
              <option value={entry.name}>{entry.name}</option>
            {/each}
          </select>
        </label>
      </div>
      {#if shareInfo?.acl}
        <p class="subtle">{t('admin-share-acl')}</p>
      {/if}

      <div class="table" role="table" aria-label={t('admin-accounts')}>
        <div class="row head" role="row">
          <span role="columnheader">{t('admin-account')}</span>
          <span role="columnheader">{t('admin-sftp')}</span>
          <span role="columnheader">{t('admin-permission', { share })}</span>
          <span role="columnheader">{t('admin-ready')}</span>
        </div>
        {#each overview.accounts as account (account.name)}
          <div class="row" role="row" class:disabled={account.disabled}>
            <span role="cell" class="who">
              <strong>{account.name}</strong>
              <span class="badges">
                {#if account.admin}<span class="badge">{t('admin-badge-admin')}</span>{/if}
                {#if account.disabled}<span class="badge danger">{t('admin-badge-disabled')}</span
                  >{/if}
              </span>
              {#if account.description}<span class="subtle">{account.description}</span>{/if}
            </span>
            <span role="cell">
              <select
                class="select"
                value={sftpChoice(account)}
                aria-label={t('admin-sftp')}
                onchange={(event) =>
                  setSftp(account, (event.currentTarget as HTMLSelectElement).value as SftpChoice)}
              >
                <option value="default">{t('admin-sftp-default')}</option>
                <option value="allowed">{t('admin-sftp-allowed')}</option>
                <option value="denied">{t('admin-sftp-denied')}</option>
              </select>
            </span>
            <span role="cell">
              {#if permissionOf(account.name) === 'custom'}
                <span class="subtle">{t('admin-permission-custom')}</span>
              {:else}
                <select
                  class="select"
                  aria-label={t('admin-permission', { share })}
                  onchange={(event) =>
                    setPermission(
                      account.name,
                      (event.currentTarget as HTMLSelectElement).value as Permission,
                    )}
                >
                  {#each permissions as permission (permission)}
                    <option
                      value={permission}
                      selected={permission === permissionChoice(account.name)}
                    >
                      {t(
                        permission === 'none' && isAdmin(account.name)
                          ? 'admin-permission-via-admins'
                          : `admin-permission-${permission}`,
                      )}
                    </option>
                  {/each}
                </select>
              {/if}
            </span>
            <span role="cell">
              {#if ready(account)}
                <span class="badge success">{t('admin-ready-yes')}</span>
              {:else}
                <span class="badge">{t('admin-ready-no')}</span>
              {/if}
            </span>
          </div>
        {/each}
      </div>
      <p class="subtle">{t('admin-note')}</p>

      {#if failure}
        <p class="failure" role="alert">
          {t(`error-${failure.key}`)}
          {#if failure.detail}<span class="subtle block">{failure.detail}</span>{/if}
        </p>
      {/if}
      {#if saved && changeCount === 0}
        <p class="subtle" role="status">{t('admin-saved')}</p>
      {/if}
      <div class="actions">
        <button
          class="button"
          disabled={changeCount === 0 || busy}
          onclick={() => {
            sftpEdits = {};
            shareEdits = {};
          }}
        >
          {t('admin-discard')}
        </button>
        <button
          class="button primary"
          disabled={changeCount === 0 || busy}
          onclick={() => (confirming = true)}
        >
          {t('admin-apply', { count: changeCount })}
        </button>
      </div>
    </section>
  {:else if failure}
    <p class="failure" role="alert">{t(`error-${failure.key}`)}</p>
  {/if}
</div>

<Dialog bind:open={confirming} title={t('admin-confirm-title')}>
  <p class="muted">{t('admin-confirm-text', { name: overview?.device.name ?? '' })}</p>
  <ul class="changes">
    {#each changes.sftp as change (change.account)}
      <li>
        {t('admin-change-sftp', { account: change.account, value: describeSftp(change.access) })}
      </li>
    {/each}
    {#each changes.shares as change (change.share + change.account)}
      <li>
        {t('admin-change-share', {
          account: change.account,
          share: change.share,
          value: t(`admin-permission-${change.permission}`),
        })}
      </li>
    {/each}
  </ul>
  {#snippet actions()}
    <button class="button" onclick={() => (confirming = false)}>{t('settings-cancel')}</button>
    <button class="button primary" disabled={busy} onclick={apply}>{t('admin-confirm')}</button>
  {/snippet}
</Dialog>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .title-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .services {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 24px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .services li {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .services :global(.ok) {
    color: var(--success);
  }

  .services :global(.bad) {
    color: var(--danger);
  }

  .share-picker {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .table {
    display: flex;
    flex-direction: column;
  }

  .row {
    display: grid;
    grid-template-columns: minmax(140px, 1.4fr) minmax(130px, 1fr) minmax(150px, 1.1fr) 7.5em;
    align-items: center;
    gap: 12px;
    padding: 8px 2px;
  }

  .row + .row {
    border-top: 1px solid var(--card-border);
  }

  .row.head {
    color: var(--text-3);
    font-size: 0.88em;
    font-weight: 600;
  }

  .row.disabled .who {
    opacity: 0.6;
  }

  .row .select {
    width: 100%;
  }

  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.3;
  }

  .badges {
    display: flex;
    gap: 4px;
  }

  .changes {
    margin: 0;
    padding-left: 20px;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
  }

  @media (max-width: 760px) {
    .row {
      grid-template-columns: 1fr 1fr;
    }

    .row.head {
      display: none;
    }
  }
</style>
