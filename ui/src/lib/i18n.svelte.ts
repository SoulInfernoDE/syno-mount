// Fluent catalogs shared with the Rust core (`/locales/<locale>/*.ftl`).
// English is the base locale and always ends the fallback chain.

import { FluentBundle, FluentResource, type FluentVariable } from '@fluent/bundle';
import { negotiateLanguages } from '@fluent/langneg';

export const FALLBACK_LOCALE = 'en';

const sources = import.meta.glob<string>('../../../locales/*/*.ftl', {
  query: '?raw',
  import: 'default',
  eager: true,
});

const catalogs = new Map<string, string[]>();
for (const [path, source] of Object.entries(sources)) {
  const locale = path.split('/').at(-2);
  if (!locale) continue;
  catalogs.set(locale, [...(catalogs.get(locale) ?? []), source]);
}

export const availableLocales: readonly string[] = [...catalogs.keys()].sort();

const bundles = new Map(
  availableLocales.map((locale) => {
    const bundle = new FluentBundle(locale);
    for (const source of catalogs.get(locale) ?? []) {
      for (const error of bundle.addResource(new FluentResource(source))) {
        console.error(`[i18n] ${locale}:`, error);
      }
    }
    return [locale, bundle] as const;
  }),
);

function resolveChain(requested: readonly string[]): FluentBundle[] {
  const order = negotiateLanguages(requested, availableLocales, {
    defaultLocale: FALLBACK_LOCALE,
    strategy: 'filtering',
  }).filter((locale) => locale !== FALLBACK_LOCALE);
  order.push(FALLBACK_LOCALE);
  return order.flatMap((locale) => bundles.get(locale) ?? []);
}

let chain = $state(resolveChain(navigator.languages));

/** The most preferred locale currently in use. */
export function currentLocale(): string {
  return chain[0]?.locales[0] ?? FALLBACK_LOCALE;
}

/** `language` is the user's choice; `null` follows the system (browser) languages. */
export function applyLanguage(language: string | null): void {
  // Works on the new chain only, so callers inside an effect do not subscribe to `chain`.
  const next = resolveChain(language ? [language] : navigator.languages);
  chain = next;
  document.documentElement.lang = next[0]?.locales[0] ?? FALLBACK_LOCALE;
}

/** Formats `id` with the first locale that defines it; falls back to the raw ID. */
export function t(id: string, args?: Record<string, FluentVariable>): string {
  for (const bundle of chain) {
    const pattern = bundle.getMessage(id)?.value;
    if (pattern) return bundle.formatPattern(pattern, args);
  }
  return id;
}

/** Native display name of a locale, e.g. "Deutsch" for `de`. */
export function localeName(locale: string): string {
  return new Intl.DisplayNames([locale], { type: 'language' }).of(locale) ?? locale;
}

document.documentElement.lang = currentLocale();
