// 剑网三专栏数据：产品站上的 column.json 也是随包快照，写错一处就是线上和离线一起错，所以钉在这里。
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import {
  cloudMacroName,
  type Column,
  validateColumn,
} from '../apps/main/src/windows/panel/jx3/column.ts';
import {
  type ColumnResult,
  defaultSchoolId,
  entriesFor,
  schoolGroups,
  sourceLine,
  sourceSite,
  splitByAuthorization,
} from '../apps/main/src/windows/panel/jx3/view.ts';
import { SECT_PRESETS } from '../apps/main/src/windows/panel/theme.ts';

const COLUMN_PATH = fileURLToPath(
  new URL('../apps/site/public/jx3/column.json', import.meta.url),
);
const load = (): Column => JSON.parse(readFileSync(COLUMN_PATH, 'utf8'));

test('column.json 通过校验', () => {
  assert.deepEqual(validateColumn(load()), []);
});

test('主题预设的每个门派与流派都在表里，打开专栏时能按主题默认选中', () => {
  const schools = new Set(load().schools.map((s) => s.id));
  for (const preset of SECT_PRESETS) assert.ok(schools.has(preset.id), `缺 ${preset.sect}`);
});

test('无相楼是流派，不和门派混在一起', () => {
  const wuxiang = load().schools.find((s) => s.id === 'wuxiang');
  assert.equal(wuxiang?.kind, 'genre');
});

test('首条授权条目：花间游，轻尘', () => {
  const paper = load().whitepapers.find((w) => w.url === 'https://www.jx3box.com/bps/110519');
  assert.equal(paper?.spec, '花间游');
  assert.equal(paper?.author, '轻尘');
});

test('心法写错、http 链接、授权记录写错或只写一半都会被拦下', () => {
  const broken = load();
  const { authorized_note: _, ...halfRecord } = broken.whitepapers[0];
  broken.whitepapers.push(
    { ...broken.whitepapers[0], url: 'https://example.com/a', spec: '不存在的心法' },
    { ...broken.whitepapers[0], url: 'http://example.com/b' },
    { ...broken.whitepapers[0], url: 'https://example.com/c', authorized_at: '' },
    { ...broken.whitepapers[0] },
    { ...halfRecord, url: 'https://example.com/d' },
  );
  const errors = validateColumn(broken);
  assert.ok(errors.some((e) => e.includes('whitepapers[1].spec')));
  assert.ok(errors.some((e) => e.includes('whitepapers[2].url')));
  assert.ok(errors.some((e) => e.includes('whitepapers[3].authorized_at')));
  assert.ok(errors.some((e) => e.includes('whitepapers[4] 重复')));
  assert.ok(errors.some((e) => e.includes('whitepapers[5] 授权记录不完整')));
});

test('没有授权记录的公开帖是合法条目，界面上折叠在授权条目之后', () => {
  const data = load();
  data.whitepapers.push({
    spec: '花间游',
    title: '某篇公开攻略',
    author: '某作者',
    url: 'https://www.jx3box.com/bps/1',
  });
  assert.deepEqual(validateColumn(data), []);
  const { authorized, others } = splitByAuthorization(entriesFor(data.whitepapers, '花间游'));
  assert.equal(authorized[0].author, '轻尘');
  assert.deepEqual(
    others.map((e) => e.title),
    ['某篇公开攻略'],
  );
});

test('来源站点：认得的写站名，其余写域名', () => {
  assert.equal(sourceSite('https://www.jx3box.com/bps/1'), 'JX3Box');
  assert.equal(sourceSite('https://bbs.nga.cn/read.php?tid=1'), 'NGA');
  assert.equal(sourceSite('https://www.example.org/a'), 'example.org');
});

test('宏帖子校验类型、宏名、重复帖子与撞名的云端宏', () => {
  const data = load();
  const base = data.macros.length;
  const macro = {
    spec: '花间游',
    kind: 'multi-stage' as const,
    title: '某篇宏',
    author: '某作者',
    url: 'https://www.jx3box.com/macro/1',
    items: [{ name: '起手宏' }, { name: '循环宏' }],
  };
  const url = (n: number) => `https://www.jx3box.com/macro/${n}`;
  data.macros.push(
    macro,
    { ...macro, url: url(2), kind: 'other' as never, items: [{ name: '爆发宏' }] },
    { ...macro, items: [{ name: '另一个' }] },
    { ...macro, url: url(3), items: [{ name: '循环宏' }] },
    { ...macro, url: url(4), items: [] },
    { ...macro, url: url(5), items: [{ name: 'a' }, { name: 'a' }] },
  );
  const at = (i: number) => `macros[${base + i}]`;
  const errors = validateColumn(data);
  assert.ok(!errors.some((e) => e.startsWith(`${at(0)}`)));
  assert.ok(errors.some((e) => e.includes(`${at(1)}.kind`)));
  assert.ok(errors.some((e) => e.includes(`${at(2)} 重复`)));
  assert.ok(errors.some((e) => e.includes(`${at(3)}.items[0] 云端宏名重复：某作者#循环宏`)));
  assert.ok(errors.some((e) => e.includes(`${at(4)}.items 不能为空`)));
  assert.ok(errors.some((e) => e.includes(`${at(5)}.items[1].name 重复`)));
});

test('宏正文只能跟着作者授权的条目，没授权的公开帖只给云端宏名', () => {
  const data = load();
  const base = data.macros.length;
  const macro = {
    spec: '花间游',
    kind: 'multi-stage' as const,
    title: '某篇宏',
    author: '某作者',
    url: 'https://www.jx3box.com/macro/1',
    items: [{ name: '循环宏', body: '/cast 阳明指' }],
  };
  data.macros.push(macro, {
    ...macro,
    url: 'https://www.jx3box.com/macro/2',
    authorized_at: '2026-10-29',
    authorized_note: '测试',
  });
  const errors = validateColumn(data);
  assert.ok(errors.some((e) => e.includes(`macros[${base}].items[0].body 只有作者授权`)));
  assert.ok(!errors.some((e) => e.startsWith(`macros[${base + 1}]`)));
});

test('轻尘白皮书里的三个宏随授权收录正文', () => {
  const post = load().macros.find((m) => m.author === '轻尘')!;
  assert.equal(post.url, 'https://www.jx3box.com/bps/110519');
  assert.ok(post.authorized_at);
  assert.equal(post.items.length, 3);
  assert.ok(post.items.every((m) => m.body?.startsWith('/cast ')));
});

test('格式版本与顶层结构', () => {
  assert.deepEqual(validateColumn(null), ['顶层必须是对象']);
  assert.ok(validateColumn({ ...load(), schema: 2 }).some((e) => e.startsWith('schema')));
});

test('云端宏名是 作者#宏名，与 JX3Box「复制云端宏」一致', () => {
  const post = load().macros.find((m) => m.url === 'https://www.jx3box.com/macro/96984')!;
  assert.deepEqual(
    post.items.map((m) => cloudMacroName(post.author, m.name)),
    ['蔓海赋尘#太极秘录循环宏', '蔓海赋尘#起手宏太极秘录'],
  );
});

test('默认门派跟着主题色走，流派同样认，对不上就选第一个门派', () => {
  const data = load();
  const color = (id: string) => SECT_PRESETS.find((p) => p.id === id)!.color;
  assert.equal(defaultSchoolId(data, color('wanhua').toUpperCase()), 'wanhua');
  assert.equal(defaultSchoolId(data, color('wuxiang')), 'wuxiang');
  assert.equal(defaultSchoolId(data, '#123456'), data.schools.find((s) => s.kind === 'sect')!.id);
});

test('门派与流派分组，按心法取条目', () => {
  const data = load();
  const { sects, genres } = schoolGroups(data);
  assert.ok(sects.every((s) => s.kind === 'sect'));
  assert.deepEqual(
    genres.map((s) => s.id),
    ['wuxiang'],
  );
  assert.equal(entriesFor(data.whitepapers, '花间游').length, 1);
  assert.deepEqual(entriesFor(data.whitepapers, '离经易道'), []);
  assert.deepEqual(entriesFor(data.whitepapers, undefined), []);
});

test('来源说明：线上只写更新日期，回落时写原因和用的是哪份', () => {
  const base: ColumnResult = { data: load(), source: 'remote', fetched_at: null, fallback: null };
  assert.equal(sourceLine(base), `列表更新于 ${base.data.updated_at}`);
  const bundled = sourceLine({ ...base, source: 'bundled', fallback: 'offline' });
  assert.match(bundled, /^暂时连不上官网，显示安装包内置的列表/);
  const newer = sourceLine({ ...base, source: 'cache', fetched_at: 1_761_700_000, fallback: 'newer_schema' });
  assert.match(newer, /^官网的列表格式已更新，请更新应用.*显示 \d{4}-\d{2}-\d{2} 取到的列表/);
});
