import { readFileSync, writeFileSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const raw = process.argv[2];
if (!raw) {
  console.error('用法：pnpm bump-version <X.Y.Z>');
  process.exit(1);
}
const version = raw.replace(/^v/, '');
if (!/^\d+\.\d+\.\d+$/.test(version)) {
  console.error(`版本号格式错误：${raw}，需为 X.Y.Z`);
  process.exit(1);
}

interface FileUpdater {
  path: string;
  currentVersion(content: string): string | undefined;
  update(content: string): string;
}

const today = new Date().toISOString().split('T')[0];

const files: FileUpdater[] = [
  {
    path: 'apps/main/package.json',
    currentVersion: (c) => (JSON.parse(c) as { version: string }).version,
    update(c) {
      const obj = JSON.parse(c) as Record<string, unknown>;
      obj.version = version;
      return JSON.stringify(obj, null, 2) + '\n';
    },
  },
  {
    path: 'apps/main/src-tauri/tauri.conf.json',
    currentVersion: (c) => (JSON.parse(c) as { version: string }).version,
    update(c) {
      const obj = JSON.parse(c) as Record<string, unknown>;
      obj.version = version;
      return JSON.stringify(obj, null, 2) + '\n';
    },
  },
  {
    path: 'Cargo.toml',
    currentVersion: (c) => c.match(/^version = "(.*)"/m)?.[1],
    update: (c) => c.replace(/^version = ".*"$/m, `version = "${version}"`),
  },
  {
    path: 'CHANGELOG.md',
    currentVersion: (c) => (c.includes(`## [${version}]`) ? version : undefined),
    update(c) {
      const UNRELEASED = '## [Unreleased]';
      const newSection = `${UNRELEASED}\n\n## [${version}] - ${today}`;
      return c.includes(UNRELEASED)
        ? c.replace(UNRELEASED, newSection)
        : c.replace(/^(## \[)/, `## [${version}] - ${today}\n\n$1`);
    },
  },
];

for (const { path, currentVersion, update } of files) {
  const abs = resolve(root, path);
  const before = readFileSync(abs, 'utf8');
  if (currentVersion(before) === version) {
    console.log(`  跳过 ${path}（已是 v${version}）`);
    continue;
  }
  writeFileSync(abs, update(before));
  console.log(`  已更新 ${path}`);
}

console.log(`\n版本号已升至 v${version}`);
console.log(`提示：在 CHANGELOG.md 的 [${version}] 节填写本次更新内容后再提交`);
console.log(`提交：git add -p && git commit -m "chore(release): bump version to ${version}"`);
