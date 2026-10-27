<script lang="ts">
  import icon from '../../../assets/branding/icon.svg';
  import { availableLocales, localeName, t } from '../lib/i18n.svelte';
  import {
    settings,
    systemSkin,
    type SchemeChoice,
    type Skin,
    type SkinChoice,
  } from '../lib/settings.svelte';

  // Platform names are proper names and need no translation.
  const skinNames: Record<Skin, string> = { linux: 'Linux', macos: 'macOS', windows: 'Windows' };

  const skins: { value: SkinChoice; label: () => string }[] = [
    { value: 'auto', label: () => t('settings-skin-auto', { skin: skinNames[systemSkin()] }) },
    { value: 'linux', label: () => skinNames.linux },
    { value: 'macos', label: () => skinNames.macos },
    { value: 'windows', label: () => skinNames.windows },
  ];

  const schemes: { value: SchemeChoice; label: string }[] = [
    { value: 'auto', label: 'settings-scheme-auto' },
    { value: 'light', label: 'settings-scheme-light' },
    { value: 'dark', label: 'settings-scheme-dark' },
  ];
</script>

<div class="page">
  <h1>{t('app-settings-title')}</h1>

  <section class="card">
    <h2>{t('app-settings-appearance')}</h2>

    <div class="row">
      <span id="skin-label">{t('settings-skin')}</span>
      <div class="segmented" role="radiogroup" aria-labelledby="skin-label">
        {#each skins as skin (skin.value)}
          <label class:active={settings.skin === skin.value}>
            <input type="radio" name="skin" value={skin.value} bind:group={settings.skin} />
            {skin.label()}
          </label>
        {/each}
      </div>
    </div>

    <div class="row">
      <span id="scheme-label">{t('settings-scheme')}</span>
      <div class="segmented" role="radiogroup" aria-labelledby="scheme-label">
        {#each schemes as scheme (scheme.value)}
          <label class:active={settings.scheme === scheme.value}>
            <input type="radio" name="scheme" value={scheme.value} bind:group={settings.scheme} />
            {t(scheme.label)}
          </label>
        {/each}
      </div>
    </div>

    <div class="row">
      <label for="language">{t('settings-language')}</label>
      <select id="language" class="select" bind:value={settings.language}>
        <option value={null}>{t('settings-language-auto')}</option>
        {#each availableLocales as locale (locale)}
          <option value={locale}>{localeName(locale)}</option>
        {/each}
      </select>
    </div>
  </section>

  <section class="card">
    <h2>{t('settings-files')}</h2>
    <p class="muted selectable">{t('settings-files-text')}</p>
  </section>

  <section class="card about">
    <img src={icon} alt="" width="48" height="48" />
    <div>
      <strong>Syno Mount</strong>
      <p class="subtle">{t('settings-version', { version: __APP_VERSION__ })}</p>
      <p class="subtle">{t('settings-license')}</p>
      <p class="subtle">{t('unofficial')}</p>
    </div>
  </section>
</div>

<style>
  h2 {
    margin-bottom: 6px;
    font-size: 1em;
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 10px 0;
  }

  .row + .row {
    border-top: 1px solid var(--card-border);
  }

  .about {
    display: flex;
    gap: 14px;
    align-items: flex-start;
  }

  @media (max-width: 640px) {
    .row {
      flex-direction: column;
      align-items: flex-start;
    }
  }
</style>
