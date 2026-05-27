import { appendFileSync, readFileSync } from 'fs';
import { dirname, resolve } from 'path';
import { fileURLToPath } from 'url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const rawTag = process.argv[2];
if (!rawTag) {
  console.error('用法：tsx scripts/extract-changelog.ts <vX.Y.Z> [<上一次成功发布的 vX.Y.Z>]');
  process.exit(1);
}
const version = rawTag.replace(/^v/, '');

const rawPrev = process.argv[3]?.trim() ?? '';
const prevVersion =
  rawPrev && rawPrev.replace(/^v/, '') !== version ? rawPrev.replace(/^v/, '') : undefined;

const lines = readFileSync(resolve(root, 'CHANGELOG.md'), 'utf8').split('\n');

const startIdx = lines.findIndex((line) => line.startsWith(`## [${version}]`));
if (startIdx === -1) {
  console.error(`CHANGELOG.md 中未找到版本 ${version} 的记录`);
  process.exit(1);
}

let endIdx: number;
if (prevVersion) {
  endIdx = lines.findIndex((line, i) => i > startIdx && line.startsWith(`## [${prevVersion}]`));
  if (endIdx === -1) {
    console.warn(
      `警告：CHANGELOG.md 中未找到上一次发布版本 ${prevVersion}，回退到只提取 ${version} 单节`,
    );
    endIdx = lines.findIndex((line, i) => i > startIdx && line.startsWith('## ['));
  }
} else {
  endIdx = lines.findIndex((line, i) => i > startIdx && line.startsWith('## ['));
}
if (endIdx === -1) endIdx = lines.length;

const body = lines
  .slice(startIdx + 1, endIdx)
  .join('\n')
  .trim();

const result =
  body +
  '\n\n---\n完整更新历史见 [CHANGELOG.md](https://github.com/x-wink/flair-bloom/blob/main/CHANGELOG.md)';

const outputFile = process.env['GITHUB_OUTPUT'];
if (outputFile) {
  appendFileSync(outputFile, `body<<CHANGELOG_EOF\n${result}\nCHANGELOG_EOF\n`);
} else {
  console.log(result);
}
