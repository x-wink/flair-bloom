import { appendFileSync, readFileSync } from 'fs';
import { dirname, resolve } from 'path';
import { fileURLToPath } from 'url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const tag = process.argv[2];
if (!tag) {
  console.error('用法：tsx scripts/extract-changelog.ts <vX.Y.Z>');
  process.exit(1);
}
const version = tag.replace(/^v/, '');

const lines = readFileSync(resolve(root, 'CHANGELOG.md'), 'utf8').split('\n');
let inSection = false;
const body: string[] = [];

for (const line of lines) {
  if (line.startsWith('## [')) {
    if (inSection) break;
    if (line.startsWith(`## [${version}]`)) inSection = true;
    continue;
  }
  if (inSection) body.push(line);
}

if (!inSection) {
  console.error(`CHANGELOG.md 中未找到版本 ${version} 的记录`);
  process.exit(1);
}

const result =
  body.join('\n').trim() +
  '\n\n---\n完整更新历史见 [CHANGELOG.md](https://github.com/x-wink/flair-bloom/blob/main/CHANGELOG.md)';

const outputFile = process.env['GITHUB_OUTPUT'];
if (outputFile) {
  appendFileSync(outputFile, `body<<CHANGELOG_EOF\n${result}\nCHANGELOG_EOF\n`);
} else {
  console.log(result);
}
