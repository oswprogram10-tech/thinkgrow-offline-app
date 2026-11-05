// Verifies the offline stylesheet keeps the shared design tokens in sync and
// that every built application ships the theme helper. No browser, no network.
import assert from 'node:assert/strict';
import {existsSync, readFileSync} from 'node:fs';
import {dirname, join} from 'node:path';
import {fileURLToPath} from 'node:url';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const tokens = JSON.parse(readFileSync(join(root, 'docs/design/tokens.json'), 'utf8'));
const css = readFileSync(join(root, 'apps/shared-ui/app.css'), 'utf8');

function block(source, selector) {
  const pattern = new RegExp(selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + '\\s*\\{([^}]*)\\}');
  const match = source.match(pattern);
  assert.ok(match, `missing CSS block: ${selector}`);
  const declarations = {};
  for (const [, name, value] of match[1].matchAll(/(--[A-Za-z0-9-]+)\s*:\s*([^;]+);/g)) {
    declarations[name] = value.replace(/\s+/g, '');
  }
  return declarations;
}

const light = block(css, ':root');
const dark = block(css, ':root[data-theme="dark"]');

const expectedLight = {
  ...tokens.fixed,
  ...Object.fromEntries(Object.entries(tokens.themed).map(([name, value]) => [name, value.light])),
};

for (const [name, value] of Object.entries(expectedLight)) {
  assert.ok(name in light, `light token missing: ${name}`);
  assert.equal(light[name], String(value).replace(/\s+/g, ''), `light token mismatch: ${name}`);
}

for (const [name, value] of Object.entries(tokens.themed)) {
  assert.ok(name in dark, `dark token missing: ${name}`);
  assert.equal(dark[name], String(value.dark).replace(/\s+/g, ''), `dark token mismatch: ${name}`);
}

// Only the six role accents may use raw hex values outside the token blocks.
const withoutTokens = css
  .replace(/:root\s*\{[^}]*\}/, '')
  .replace(/:root\[data-theme="dark"\]\s*\{[^}]*\}/, '')
  .replace(/:root\[data-theme="dark"\] body\[data-role="[a-z]+"\]\s*\{[^}]*\}/g, '')
  .replace(/body\[data-role="[a-z]+"\]\s*\{[^}]*\}/g, '');
const strayHex = withoutTokens.match(/#[0-9a-fA-F]{3,8}\b/g) || [];
assert.deepEqual(strayHex, [], `hex colours outside tokens/role accents: ${strayHex.join(', ')}`);

const roles = ['teacher', 'student', 'parent'];
for (const role of roles) {
  const built = join(root, 'target/ui', role);
  assert.ok(existsSync(join(built, 'theme.js')), `${role}: target/ui/${role}/theme.js missing`);
  assert.equal(readFileSync(join(built, 'theme.js'), 'utf8'),
    readFileSync(join(root, 'apps/shared-ui/theme.js'), 'utf8'), `${role}: stale theme.js`);
  const html = readFileSync(join(built, 'index.html'), 'utf8');
  assert.ok(html.includes('theme.js'), `${role}: index.html does not load theme.js`);
  assert.ok(html.includes('id="theme-toggle"'), `${role}: index.html has no theme toggle`);
  assert.ok(!/https?:\/\//.test(html), `${role}: index.html references an external URL`);
}

const report = {
  status: 'passed',
  checked: {
    light_tokens: Object.keys(expectedLight).length,
    dark_tokens: Object.keys(tokens.themed).length,
    role_accents: 6,
    built_roles: roles.length,
  },
};
console.log(JSON.stringify(report));
