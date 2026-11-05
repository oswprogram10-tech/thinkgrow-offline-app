import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = dirname(dirname(fileURLToPath(import.meta.url)));
const role = process.argv[2];
if (!['teacher', 'student', 'parent'].includes(role)) throw Error('Expected a fixed application role');
const output = join(root, 'target', 'ui', role);
mkdirSync(output, { recursive: true });
for (const [name, source] of [['index.html', join(root, 'apps', role, 'ui', 'index.html')], ['app.css', join(root, 'apps/shared-ui/app.css')], ['theme.js', join(root, 'apps/shared-ui/theme.js')], ['app.js', join(root, 'apps/shared-ui/app.js')]]) {
  writeFileSync(join(output, name), readFileSync(source));
}
console.log('Built local UI: ' + role);

if (role === "student") writeFileSync(join(output,"student.js"),readFileSync(join(root,"apps/student/ui/student.js")));
for(const name of ["roles.js", ...(role === "parent" ? [] : ["games.js", "materials.js"])]) writeFileSync(join(output,name),readFileSync(join(root,"apps/shared-ui",name)));
