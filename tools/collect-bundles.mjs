#!/usr/bin/env node
// Copies the packages `tauri build` made into one folder, under stable file names
// without spaces, e.g. syno-mount-0.5.0-x86_64.AppImage.
//
//   node tools/collect-bundles.mjs <bundle dir> <output dir>

import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const [bundleDir, outDir] = process.argv.slice(2);
if (!bundleDir || !outDir) {
  console.error('usage: collect-bundles.mjs <bundle dir> <output dir>');
  process.exit(2);
}

const cargo = readFileSync(new URL('../Cargo.toml', import.meta.url), 'utf8');
const version = /^\[workspace\.package\][^[]*?^version = "([^"]+)"/ms.exec(cargo)?.[1];
if (!version) throw new Error('workspace version not found in Cargo.toml');

const arm = process.arch === 'arm64';
const names = {
  deb: () => `syno-mount_${version}_${arm ? 'arm64' : 'amd64'}.deb`,
  rpm: () => `syno-mount-${version}-1.${arm ? 'aarch64' : 'x86_64'}.rpm`,
  appimage: () => `syno-mount-${version}-${arm ? 'aarch64' : 'x86_64'}.AppImage`,
};
const extensions = {
  deb: '.deb',
  rpm: '.rpm',
  appimage: '.AppImage',
};

mkdirSync(outDir, { recursive: true });
let copied = 0;
for (const [kind, name] of Object.entries(names)) {
  const dir = join(bundleDir, kind);
  if (!existsSync(dir)) continue;
  const files = readdirSync(dir).filter((file) => file.endsWith(extensions[kind]));
  if (files.length !== 1)
    throw new Error(`expected one ${kind} package in ${dir}, found ${files.length}`);
  copyFileSync(join(dir, files[0]), join(outDir, name()));
  console.log(`${kind}: ${files[0]} -> ${name()}`);
  copied += 1;
}
if (copied === 0) throw new Error(`no packages found in ${bundleDir}`);
