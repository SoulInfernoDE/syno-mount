<script lang="ts">
  import Dialog from './Dialog.svelte';
  import SelfTest from './SelfTest.svelte';
  import {
    commandLine,
    parseAddress,
    removeMount,
    selfTest,
    speedTest,
    toFailure,
    updateMount,
    type Cache,
    type Check,
    type CpuKind,
    type Failure,
    type Mount,
    type Speed,
  } from '../lib/backend';
  import { bytes, count, rate } from '../lib/format';
  import { t } from '../lib/i18n.svelte';
  import type { MountAction } from '../lib/nav';

  let {
    mount,
    action = $bindable(),
    onchanged,
  }: {
    mount: Mount;
    /** The open dialog; `null` closes it. */
    action: MountAction | null;
    onchanged: () => void;
  } = $props();

  let open = $state(true);
  let busy = $state(false);
  let failure = $state<Failure | null>(null);

  $effect(() => {
    if (!open) action = null;
  });

  function close() {
    open = false;
  }

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

  // Self-test
  let checks = $state<Check[] | null>(null);
  async function test() {
    checks = null;
    checks = (await run(() => selfTest(mount.id))) ?? null;
  }

  // Speed test
  let size = $state(128);
  let speed = $state<Speed | null>(null);
  async function measure() {
    speed = null;
    speed = (await run(() => speedTest(mount.id, size))) ?? null;
  }

  // Command
  let command = $state('');
  let copied = $state(false);
  async function loadCommand() {
    command = (await run(() => commandLine(mount.id))) ?? '';
  }
  async function copy() {
    await navigator.clipboard.writeText(command);
    copied = true;
  }

  // Settings: the form starts from the mount once; later edits belong to the user.
  // svelte-ignore state_referenced_locally
  let atLogin = $state(mount.atLogin);
  // svelte-ignore state_referenced_locally
  let bookmark = $state(mount.bookmark);
  // svelte-ignore state_referenced_locally
  let internet = $state(mount.internet ? `${mount.internet.host}:${mount.internet.port}` : '');
  // svelte-ignore state_referenced_locally
  let cpu = $state<CpuKind>(mount.tuning.cpu);
  // svelte-ignore state_referenced_locally
  let connections = $state(mount.tuning.connections);
  // svelte-ignore state_referenced_locally
  let compress = $state(mount.tuning.compressInternet);
  // svelte-ignore state_referenced_locally
  let cache = $state<Cache>(mount.tuning.cache);
  let saved = $state(false);
  const internetAddress = $derived(internet.trim() ? parseAddress(internet, 22) : undefined);

  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (internetAddress === null) return;
    const done = await run(() =>
      updateMount(mount.id, {
        atLogin,
        bookmark,
        internet: internetAddress,
        tuning: { cpu, connections, compressInternet: compress, cache },
      }),
    );
    if (done) {
      saved = true;
      onchanged();
    }
  }

  // Remove
  let removeKey = $state(false);
  async function remove() {
    const done = await run(async () => {
      await removeMount(mount.id, removeKey);
      return true;
    });
    if (done) {
      onchanged();
      close();
    }
  }

  $effect(() => {
    if (action === 'selftest') void test();
    if (action === 'command') void loadCommand();
  });

  const title = $derived(
    {
      selftest: t('selftest-title', { share: mount.share }),
      speed: t('speed-title', { share: mount.share }),
      settings: t('settings-mount-title', { share: mount.share }),
      command: t('command-title', { share: mount.share }),
      remove: t('remove-title', { share: mount.share }),
      more: '',
    }[action ?? 'more'],
  );
</script>

{#snippet problem()}
  {#if failure}
    <p class="failure" role="alert">
      {t(`error-${failure.key}`)}
      {#if failure.detail}<span class="subtle block">{failure.detail}</span>{/if}
    </p>
  {/if}
{/snippet}

<Dialog bind:open {title} wide={action === 'selftest' || action === 'command'}>
  {#if action === 'selftest'}
    {#if busy}
      <p class="muted">{t('selftest-running')}</p>
    {:else if checks}
      <p class="subtle">{t('selftest-intro')}</p>
      <SelfTest {checks} />
    {/if}
    {@render problem()}
  {:else if action === 'speed'}
    <label class="field">
      <span>{t('speed-size')}</span>
      <select class="select" bind:value={size} disabled={busy}>
        {#each [32, 128, 512, 1024] as megabytes (megabytes)}
          <option value={megabytes}>{bytes(megabytes * 1024 * 1024, 'binary')}</option>
        {/each}
      </select>
    </label>
    {#if busy}
      <p class="muted">{t('speed-running', { size: bytes(size * 1024 * 1024, 'binary') })}</p>
    {:else if speed}
      <dl class="speed">
        <dt>{t('speed-write')}</dt>
        <dd>{rate(speed.writeBytesPerSecond)}</dd>
        <dt>{t('speed-read')}</dt>
        <dd>{rate(speed.readBytesPerSecond)}</dd>
        <dt>{t('speed-small')}</dt>
        <dd>{t('speed-small-value', { count: count(speed.smallFilesPerSecond) })}</dd>
      </dl>
      <p class="subtle">{t('speed-note')}</p>
    {/if}
    {@render problem()}
  {:else if action === 'command'}
    <p class="muted">{t('command-text')}</p>
    <pre class="command">{command}</pre>
    {@render problem()}
  {:else if action === 'settings'}
    <form id="mount-settings" class="settings" onsubmit={save}>
      <label class="check-row">
        <input type="checkbox" bind:checked={atLogin} />
        <span
          >{t('settings-at-login')}<span class="subtle block">{t('settings-at-login-hint')}</span
          ></span
        >
      </label>
      <label class="check-row">
        <input type="checkbox" bind:checked={bookmark} />
        <span
          >{t('settings-bookmark')}<span class="subtle block">{t('settings-bookmark-hint')}</span
          ></span
        >
      </label>
      <label class="field">
        <span>{t('settings-internet')}</span>
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
      <label class="field">
        <span>{t('settings-cipher')}</span>
        <select class="select" bind:value={cpu}>
          <option value="x86">{t('settings-cipher-x86')}</option>
          <option value="arm">{t('settings-cipher-arm')}</option>
          <option value="unknown">{t('settings-cipher-unknown')}</option>
        </select>
      </label>
      <label class="field">
        <span>{t('settings-connections')}</span>
        <select class="select" bind:value={connections}>
          {#each [1, 2, 4, 8] as value (value)}<option {value}>{value}</option>{/each}
        </select>
        <span class="subtle">{t('settings-connections-hint')}</span>
      </label>
      <label class="check-row">
        <input type="checkbox" bind:checked={compress} />
        <span
          >{t('settings-compress')}<span class="subtle block">{t('settings-compress-hint')}</span
          ></span
        >
      </label>
      <label class="field">
        <span>{t('settings-cache')}</span>
        <select class="select" bind:value={cache}>
          <option value="normal">{t('settings-cache-normal')}</option>
          <option value="fresh">{t('settings-cache-fresh')}</option>
        </select>
      </label>
      {#if saved}<p class="subtle" role="status">{t('settings-saved')}</p>{/if}
      {@render problem()}
    </form>
  {:else if action === 'remove'}
    <p class="muted">{t('remove-text')}</p>
    <label class="check-row">
      <input type="checkbox" bind:checked={removeKey} />
      <span>{t('remove-key')}<span class="subtle block">{t('remove-key-hint')}</span></span>
    </label>
    {@render problem()}
  {/if}

  {#snippet actions()}
    {#if action === 'selftest'}
      <button class="button" disabled={busy} onclick={test}>{t('selftest-again')}</button>
      <button class="button primary" onclick={close}>{t('selftest-close')}</button>
    {:else if action === 'speed'}
      <button class="button" onclick={close}>{t('speed-close')}</button>
      <button class="button primary" disabled={busy} onclick={measure}>{t('speed-start')}</button>
    {:else if action === 'command'}
      <button class="button" disabled={!command} onclick={copy}>
        {copied ? t('command-copied') : t('command-copy')}
      </button>
      <button class="button primary" onclick={close}>{t('command-close')}</button>
    {:else if action === 'settings'}
      <button class="button" onclick={close}>{t('settings-cancel')}</button>
      <button class="button primary" form="mount-settings" disabled={busy}
        >{t('settings-save')}</button
      >
    {:else if action === 'remove'}
      <button class="button" onclick={close}>{t('remove-cancel')}</button>
      <button class="button danger" disabled={busy} onclick={remove}>{t('remove-confirm')}</button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .invalid {
    color: var(--danger);
    font-size: 0.92em;
  }

  .speed {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px 18px;
    margin: 0;
  }

  .speed dt {
    color: var(--text-2);
  }

  .speed dd {
    margin: 0;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
</style>
