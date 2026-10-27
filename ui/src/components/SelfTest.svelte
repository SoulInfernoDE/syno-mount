<script lang="ts">
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import type { Check } from '../lib/backend';
  import { t } from '../lib/i18n.svelte';

  let { checks }: { checks: Check[] } = $props();

  /** The explanation of a check, when the catalog has one for its outcome. */
  function explain(check: Check): string | null {
    const id = `check-${check.id}-${check.outcome}`;
    const text = t(id);
    return text === id ? null : text;
  }
</script>

<ul>
  {#each checks as check (check.id)}
    {@const text = explain(check)}
    <li>
      <span class="icon {check.outcome}">
        {#if check.outcome === 'works'}
          <CircleCheck size={18} aria-label={t('selftest-works')} />
        {:else if check.outcome === 'limited'}
          <TriangleAlert size={18} aria-label={t('selftest-limited')} />
        {:else}
          <CircleX size={18} aria-label={t('selftest-fails')} />
        {/if}
      </span>
      <div>
        <strong>{t(`check-${check.id}`)}</strong>
        {#if text}<p class="muted">{text}</p>{/if}
        {#if check.detail}<code class="subtle">{check.detail}</code>{/if}
      </div>
    </li>
  {/each}
</ul>

<style>
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    gap: 10px;
    padding: 8px 0;
  }

  li + li {
    border-top: 1px solid var(--card-border);
  }

  .icon {
    display: flex;
    flex: none;
    margin-top: 1px;
  }

  .works {
    color: var(--success);
  }

  .limited {
    color: var(--warning);
  }

  .fails {
    color: var(--danger);
  }

  p {
    font-size: 0.94em;
  }
</style>
