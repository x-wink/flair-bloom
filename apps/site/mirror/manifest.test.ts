import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  assertSafeSegment,
  buildManifest,
  installerKind,
  planMirror,
  rewriteLatestJson,
  type GithubRelease,
} from './manifest.ts';

const digest = (seed: string) => `sha256:${seed.repeat(64).slice(0, 64)}`;

function release(
  tag: string,
  publishedAt: string,
  extra: Partial<GithubRelease> = {},
): GithubRelease {
  const version = tag.slice(1);
  const asset = (name: string, seed: string) => ({
    name,
    size: 10,
    digest: digest(seed),
    browser_download_url: `https://github.com/x-wink/flair-bloom/releases/download/${tag}/${name}`,
  });
  return {
    tag_name: tag,
    name: `FlairBloom ${tag}`,
    body: `notes ${tag}`,
    draft: false,
    prerelease: false,
    published_at: publishedAt,
    html_url: `https://github.com/x-wink/flair-bloom/releases/tag/${tag}`,
    assets: [
      asset(`FlairBloom_${version}_x64-setup.exe`, 'a'),
      asset(`FlairBloom_${version}_x64-setup.exe.sig`, 'b'),
      asset(`FlairBloom_${version}_x64_en-US.msi`, 'c'),
      asset('latest.json', 'd'),
    ],
    ...extra,
  };
}

const releases = [
  release('v0.2.9', '2026-06-23T00:00:00Z'),
  release('v0.3.1', '2026-09-09T00:00:00Z'),
  release('v0.3.2', '2026-09-20T00:00:00Z', { draft: true }),
  release('v0.3.0', '2026-06-29T00:00:00Z'),
  release('v0.4.0-beta.1', '2026-09-21T00:00:00Z', { prerelease: true }),
];

test('只镜像稳定版本里的安装包，签名文件与草稿、预发布都不进', () => {
  const plan = planMirror(releases, 'v0.3.1', { keepNotes: 10, keepAssets: 2 });
  assert.deepEqual(
    plan.mirrored.map((entry) => entry.tag_name),
    ['v0.3.1', 'v0.3.0'],
  );
  assert.deepEqual(
    plan.downloads.map((item) => `${item.tag}/${item.name}`),
    [
      'v0.3.1/FlairBloom_0.3.1_x64-setup.exe',
      'v0.3.1/FlairBloom_0.3.1_x64_en-US.msi',
      'v0.3.0/FlairBloom_0.3.0_x64-setup.exe',
      'v0.3.0/FlairBloom_0.3.0_x64_en-US.msi',
    ],
  );
  assert.equal(plan.latestJson.tag, 'v0.3.1');
});

test('Latest 指针回拨到旧版本时，该版本即使超出保留数也被镜像并记入公告', () => {
  const plan = planMirror(releases, 'v0.2.9', { keepNotes: 1, keepAssets: 1 });
  assert.deepEqual(
    plan.mirrored.map((entry) => entry.tag_name),
    ['v0.3.1', 'v0.2.9'],
  );
  const manifest = buildManifest(plan, 'https://app.xwink.fun/flair-bloom/releases/', new Date(0));
  assert.equal(manifest.latest, 'v0.2.9');
  assert.deepEqual(
    manifest.releases.map((entry) => entry.tag),
    ['v0.3.1', 'v0.2.9'],
  );
  assert.equal(
    manifest.releases[1].assets[0].url,
    'https://app.xwink.fun/flair-bloom/releases/v0.2.9/FlairBloom_0.2.9_x64-setup.exe',
  );
});

test('Latest 指向草稿或不存在的版本时整轮失败，不产出清单', () => {
  assert.throws(
    () => planMirror(releases, 'v0.3.2', { keepNotes: 5, keepAssets: 2 }),
    /不在稳定版本列表/,
  );
});

test('缺 sha256 digest 或缺更新器清单时失败', () => {
  const noDigest = release('v0.3.1', '2026-09-09T00:00:00Z');
  noDigest.assets[0].digest = null;
  assert.throws(
    () => planMirror([noDigest], 'v0.3.1', { keepNotes: 1, keepAssets: 1 }),
    /缺少 sha256/,
  );

  const noLatestJson = release('v0.3.1', '2026-09-09T00:00:00Z');
  noLatestJson.assets = noLatestJson.assets.filter((asset) => asset.name !== 'latest.json');
  assert.throws(
    () => planMirror([noLatestJson], 'v0.3.1', { keepNotes: 1, keepAssets: 1 }),
    /缺少 latest.json/,
  );
});

test('路径片段拒绝穿越与隐藏文件', () => {
  assert.equal(assertSafeSegment('v0.3.1'), 'v0.3.1');
  for (const bad of ['..', '../etc', 'a/b', '.hidden', 'v0..1', '']) {
    assert.throws(() => assertSafeSegment(bad), /不安全/);
  }
  const evil = release('v0.3.1', '2026-09-09T00:00:00Z');
  evil.assets[0].name = '../../etc/passwd-setup.exe';
  assert.throws(() => planMirror([evil], 'v0.3.1', { keepNotes: 1, keepAssets: 1 }), /不安全/);
});

test('安装包类型按文件名识别', () => {
  assert.equal(installerKind('FlairBloom_0.3.1_x64-setup.exe'), 'nsis');
  assert.equal(installerKind('FlairBloom_0.3.1_x64_en-US.msi'), 'msi');
  assert.equal(installerKind('FlairBloom_0.3.1_x64-setup.exe.sig'), undefined);
  assert.equal(installerKind('latest.json'), undefined);
});

test('更新器清单改写全部平台地址，保留签名', () => {
  const github = 'https://github.com/x-wink/flair-bloom/releases/download/v0.3.1';
  const raw = JSON.stringify({
    version: '0.3.1',
    platforms: {
      'windows-x86_64-nsis': { url: `${github}/a-setup.exe`, signature: 'sig-a' },
      'windows-x86_64-msi': { url: `${github}/b.msi`, signature: 'sig-b' },
    },
  });
  const map = new Map([
    [`${github}/a-setup.exe`, 'https://mirror/v0.3.1/a-setup.exe'],
    [`${github}/b.msi`, 'https://mirror/v0.3.1/b.msi'],
  ]);
  const rewritten = JSON.parse(rewriteLatestJson(raw, map));
  assert.equal(rewritten.platforms['windows-x86_64-nsis'].url, 'https://mirror/v0.3.1/a-setup.exe');
  assert.equal(rewritten.platforms['windows-x86_64-nsis'].signature, 'sig-a');
  assert.equal(rewritten.version, '0.3.1');

  map.delete(`${github}/b.msi`);
  assert.throws(() => rewriteLatestJson(raw, map), /未镜像的地址/);
});
