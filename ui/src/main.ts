import { mount } from 'svelte';
import App from './App.svelte';
import './styles/skins.css';
import './styles/base.css';

// The browser preview can stand in for the Rust core: `npm run dev`, then open `/?mock`.
if (import.meta.env.DEV && new URLSearchParams(location.search).has('mock')) {
  await import('./lib/mock-core');
}

const target = document.getElementById('app');
if (!target) throw new Error('missing #app mount point');

export default mount(App, { target });
