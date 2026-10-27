<script lang="ts">
  import Add from './components/Add.svelte';
  import Mounts from './components/Mounts.svelte';
  import Settings from './components/Settings.svelte';
  import Sidebar from './components/Sidebar.svelte';
  import SystemPanel from './components/SystemPanel.svelte';
  import type { Section } from './lib/nav';
  import { persistAndApply } from './lib/settings.svelte';

  let section = $state<Section>('mounts');
  /** Opens the assistant with the folders of the NAS this mount belongs to. */
  let fromMount = $state<string | undefined>();
  /** A new assistant each time the section is opened. */
  let run = $state(0);

  // Reads every setting, so it re-runs whenever one changes.
  $effect(persistAndApply);

  function add(mount?: string) {
    fromMount = mount;
    run += 1;
    section = 'add';
  }
</script>

<div class="shell">
  <Sidebar bind:section />
  <main>
    {#if section === 'mounts'}
      <Mounts onadd={add} onsystem={() => (section = 'system')} />
    {:else if section === 'add'}
      {#key run}
        <Add
          {fromMount}
          ondone={() => {
            fromMount = undefined;
            section = 'mounts';
          }}
        />
      {/key}
    {:else if section === 'system'}
      <SystemPanel />
    {:else}
      <Settings />
    {/if}
  </main>
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: var(--sidebar-width) 1fr;
    grid-template-rows: minmax(0, 1fr);
    height: 100%;
  }

  main {
    overflow-y: auto;
    padding: 28px 32px 40px;
  }

  @media (max-width: 720px) {
    .shell {
      grid-template-columns: 64px 1fr;
    }

    main {
      padding: 20px 16px 32px;
    }
  }
</style>
