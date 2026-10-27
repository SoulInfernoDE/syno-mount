#!/usr/bin/env node
// Fails when an installed npm package has a license outside the policy in deny.toml.
// Reads package-lock.json (lockfile v3 records each package's license).

import { readFileSync } from 'node:fs';

// Keep in sync with [licenses].allow in deny.toml.
const ALLOWED = new Set([
  'GPL-3.0-or-later',
  'Apache-2.0',
  'Apache-2.0 WITH LLVM-exception',
  'MIT',
  'BSD-2-Clause',
  'BSD-3-Clause',
  'ISC',
  'Zlib',
  'Unicode-3.0',
  'MPL-2.0',
  'CC0-1.0',
]);

// Accepts SPDX expressions made of allowed licenses joined by OR (any) or AND (all).
function isAllowed(expression) {
  const expr = expression.replace(/[()]/g, '').trim();
  if (expr.includes(' OR ')) return expr.split(' OR ').some(isAllowed);
  if (expr.includes(' AND ')) return expr.split(' AND ').every(isAllowed);
  return ALLOWED.has(expr);
}

const lock = JSON.parse(readFileSync(new URL('../package-lock.json', import.meta.url), 'utf8'));
const problems = [];
for (const [path, meta] of Object.entries(lock.packages)) {
  if (!path.startsWith('node_modules/') && path !== '') continue;
  if (path === '' || meta.link) continue;
  const license = typeof meta.license === 'string' ? meta.license : undefined;
  if (!license || !isAllowed(license)) problems.push(`${path}: ${license ?? 'no license field'}`);
}

if (problems.length > 0) {
  console.error(`Disallowed or missing licenses:\n  ${problems.join('\n  ')}`);
  process.exit(1);
}
console.log(`npm licenses ok (${Object.keys(lock.packages).length - 1} entries checked)`);
