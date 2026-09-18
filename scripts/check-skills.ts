import { existsSync, readdirSync, readFileSync, statSync, writeSync } from 'fs';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const TOURS_DIR = 'apps/main/src/windows/panel/tour/tours';
const MANUAL_DIR = 'skills/flair-bloom/references/manual';
const LINK_DOCS = ['skills/flair-bloom/SKILL.md', 'CLAUDE.md', 'README.md'];

// 失败信息攒起来最后一次性写出：Linux 上管道是异步写，边跑边 console.error 再 process.exit
// 会把尾部输出截断，只留下一个没有原因的非零退出码。
const errors: string[] = [];
const notes: string[] = [];

function abs(rel: string): string {
  return join(root, rel);
}

function isDir(rel: string): boolean {
  const p = abs(rel);
  return existsSync(p) && statSync(p).isDirectory();
}

/** 按「声明了 TourDef」认教程文件，不按文件名排除——目录里还会有 helpers 这类辅助模块。 */
function isTourFile(content: string): boolean {
  return /:\s*TourDef\s*=\s*\{/.test(content);
}

/** 教程 id 取 `id: 'xxx'` 字面量而不是解析 index.ts 的导出数组，导出形态变了也不影响。 */
function readTourId(content: string): string | undefined {
  return content.match(/\bid:\s*['"]([a-z0-9-]+)['"]/)?.[1];
}

function checkToursMatchManual(): void {
  if (!isDir(TOURS_DIR) || !isDir(MANUAL_DIR)) {
    notes.push(
      `⚠ 跳过教程 ↔ 说明书一致性校验：${!isDir(TOURS_DIR) ? TOURS_DIR : MANUAL_DIR} 尚不存在`,
    );
    return;
  }

  const tourIds = new Set<string>();
  for (const file of readdirSync(abs(TOURS_DIR)).filter((f) => f.endsWith('.ts'))) {
    const content = readFileSync(abs(join(TOURS_DIR, file)), 'utf8');
    if (!isTourFile(content)) continue;
    const id = readTourId(content);
    if (!id) {
      errors.push(`${TOURS_DIR}/${file}：声明了 TourDef 但没找到 id: '<kebab-case>' 字面量`);
      continue;
    }
    const expected = file.replace(/\.ts$/, '');
    if (id !== expected) {
      errors.push(`${TOURS_DIR}/${file}：id 是 '${id}'，与文件名 '${expected}' 不一致`);
    }
    tourIds.add(id);
  }

  const manualIds = new Set(
    readdirSync(abs(MANUAL_DIR))
      .filter((f) => f.endsWith('.md'))
      .map((f) => f.replace(/\.md$/, '')),
  );

  for (const id of tourIds) {
    if (!manualIds.has(id)) errors.push(`教程 '${id}' 缺少说明书 ${MANUAL_DIR}/${id}.md`);
  }
  for (const id of manualIds) {
    if (!tourIds.has(id)) errors.push(`说明书 ${MANUAL_DIR}/${id}.md 没有对应的教程 '${id}'`);
  }
}

function checkRelativeLinks(): void {
  for (const doc of LINK_DOCS) {
    if (!existsSync(abs(doc))) {
      notes.push(`⚠ 跳过 ${doc} 的链接校验：文件尚不存在`);
      continue;
    }
    const content = readFileSync(abs(doc), 'utf8');
    const docDir = dirname(abs(doc));
    for (const m of content.matchAll(/\]\(([^)\s]+)/g)) {
      const raw = m[1];
      // ../../ 起头的是 GitHub 的仓库级相对 URL（如 ](../../releases)），不是文件路径
      if (/^([a-z]+:|#|\/\/|\.\.\/\.\.\/)/i.test(raw)) continue;
      const path = decodeURI(raw.split('#')[0].split('?')[0]);
      if (!path) continue;
      if (!existsSync(resolve(docDir, path))) {
        errors.push(`${doc}：链接指向的 ${path} 不存在`);
      }
    }
  }
}

checkToursMatchManual();
checkRelativeLinks();

const lines = [...notes];
if (errors.length > 0) {
  lines.push(`✗ 文档校验失败，共 ${errors.length} 项：`, ...errors.map((e) => `  - ${e}`));
  process.exitCode = 1;
} else {
  lines.push('✓ 文档校验通过');
}
writeSync(errors.length > 0 ? 2 : 1, lines.join('\n') + '\n');
