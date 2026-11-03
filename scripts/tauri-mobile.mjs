import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, join, delimiter } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const [role, ...args] = process.argv.slice(2);
if (!['student', 'parent'].includes(role) || args.length === 0) {
  console.error('Usage: node scripts/tauri-mobile.mjs <student|parent> <tauri arguments>');
  process.exit(2);
}
const env = { ...process.env };
if (args[0] === 'android') {
  // Tauri generates package-specific Kotlin from dependency build scripts.
  // Sharing a Cargo target directory can reuse another app's generated classes.
  env.CARGO_TARGET_DIR = join(root, 'target', 'android', role);
}
if (process.platform === 'win32') {
  const cargoBin = join(env.USERPROFILE, '.cargo', 'bin');
  if (existsSync(cargoBin)) env.Path = cargoBin + delimiter + (env.Path || env.PATH || '');
}
const cli = join(root, 'node_modules', '@tauri-apps', 'cli', 'tauri.js');
const result = spawnSync(process.execPath, [cli, ...args], {
  cwd: join(root, 'apps', role), env, stdio: 'inherit',
});
if (result.error) console.error(result.error.message);
process.exit(result.status ?? 1);