// User preferences of the interface: skin, color scheme, language, expert mode.
// Kept in the WebView's local storage until the Rust core gets a settings store.

import { backdrop, hasCore, setWindowTheme } from './backend';
import { applyLanguage } from './i18n.svelte';

export type Skin = 'linux' | 'macos' | 'windows';
export type SkinChoice = Skin | 'auto';
export type SchemeChoice = 'light' | 'dark' | 'auto';

interface Stored {
  skin: SkinChoice;
  scheme: SchemeChoice;
  language: string | null;
  expert: boolean;
}

const STORAGE_KEY = 'syno-mount:settings';
const DEFAULTS: Stored = { skin: 'auto', scheme: 'auto', language: null, expert: false };

function load(): Stored {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? { ...DEFAULTS, ...(JSON.parse(raw) as Partial<Stored>) } : { ...DEFAULTS };
  } catch {
    return { ...DEFAULTS };
  }
}

/** The skin of the operating system the WebView runs on. */
export function systemSkin(): Skin {
  const platform =
    (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform ??
    navigator.userAgent;
  if (/mac/i.test(platform)) return 'macos';
  if (/win/i.test(platform)) return 'windows';
  return 'linux';
}

const darkQuery = window.matchMedia('(prefers-color-scheme: dark)');

export const settings = $state<Stored>(load());
let systemDark = $state(darkQuery.matches);
darkQuery.addEventListener('change', (event) => (systemDark = event.matches));

export function activeSkin(): Skin {
  return settings.skin === 'auto' ? systemSkin() : settings.skin;
}

export function activeScheme(): 'light' | 'dark' {
  if (settings.scheme !== 'auto') return settings.scheme;
  return systemDark ? 'dark' : 'light';
}

let windowDark: boolean | null = null;

/** Asks the core once whether the window has a backdrop such as Mica to let through. */
export async function applyBackdrop(): Promise<void> {
  if (!hasCore()) return;
  const kind = await backdrop().catch(() => null);
  if (kind) document.documentElement.dataset.backdrop = kind;
}

/** Writes the settings and applies skin, scheme and language to the document. */
export function persistAndApply(): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch {
    // Storage can be unavailable (private mode); the settings still apply for this run.
  }
  const root = document.documentElement;
  root.dataset.skin = activeSkin();
  root.dataset.scheme = activeScheme();
  // The window's own parts (title bar, Mica) follow the app's light or dark.
  const dark = activeScheme() === 'dark';
  if (hasCore() && dark !== windowDark) {
    windowDark = dark;
    void setWindowTheme(dark).catch(() => undefined);
  }
  applyLanguage(settings.language);
}
