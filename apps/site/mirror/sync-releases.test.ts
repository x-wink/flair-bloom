import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync } from 'node:fs';
import { mkdtemp, readFile, readdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, test } from 'node:test';
import type { GithubRelease } from './manifest.ts';
import { syncReleases, type SyncOptions } from './sync-releases.ts';

const BASE = 'https://app.xwink.fun/flair-bloom/releases';
const sha = (content: string) => createHash('sha256').update(content).digest('hex');

interface Fixture {
  releases: GithubRelease[];
  latest: string;
  files: Map<string, string>;
  failApi?: boolean;
  corrupt?: Set<string>;
}

function makeRelease(fixture: Fixture, tag: string, publishedAt: string): GithubRelease {
  const version = tag.slice(1);
  const url = (name: string) =>
    `https://github.com/x-wink/flair-bloom/releases/download/${tag}/${name}`;
  const exe = `FlairBloom_${version}_x64-setup.exe`;
  const msi = `FlairBloom_${version}_x64_en-US.msi`;
  const contents: Record<string, string> = {
    [exe]: `exe ${tag}`,
    [msi]: `msi ${tag}`,
    'latest.json': JSON.stringify({
      version,
      platforms: {
        'windows-x86_64': { url: url(msi), signature: 's1' },
        'windows-x86_64-nsis': { url: url(exe), signature: 's2' },
      },
    }),
  };
  for (const [name, content] of Object.entries(contents)) fixture.files.set(url(name), content);
  return {
    tag_name: tag,
    name: tag,
    body: `notes ${tag}`,
    draft: false,
    prerelease: false,
    published_at: publishedAt,
    html_url: `https://github.com/x-wink/flair-bloom/releases/tag/${tag}`,
    assets: Object.entries(contents).map(([name, content]) => ({
      name,
      size: Buffer.byteLength(content),
      digest: `sha256:${sha(content)}`,
      browser_download_url: url(name),
    })),
  };
}

function fakeFetch(fixture: Fixture, calls: string[]): typeof fetch {
  return (async (input: string | URL | Request) => {
    const url = String(input);
    calls.push(url);
    if (url.startsWith('https://api.github.com/')) {
      if (fixture.failApi) return new Response('rate limited', { status: 403 });
      if (url.endsWith('/releases/latest')) {
        return Response.json(fixture.releases.find((entry) => entry.tag_name === fixture.latest));
      }
      return Response.json(fixture.releases);
    }
    const content = fixture.files.get(url);
    if (content === undefined) return new Response('missing', { status: 404 });
    return new Response(fixture.corrupt?.has(url) ? `${content}!` : content);
  }) as typeof fetch;
}

let root: string;
let fixture: Fixture;
let calls: string[];

function options(): SyncOptions {
  return {
    root,
    repository: 'x-wink/flair-bloom',
    publicBaseUrl: BASE,
    keepNotes: 10,
    keepAssets: 2,
    fetch: fakeFetch(fixture, calls),
    now: () => new Date('2026-09-17T00:00:00Z'),
    log: () => {},
  };
}

beforeEach(async () => {
  root = await mkdtemp(join(tmpdir(), 'flair-bloom-mirror-'));
  fixture = { releases: [], latest: 'v0.3.1', files: new Map() };
  fixture.releases = [
    makeRelease(fixture, 'v0.3.1', '2026-09-09T00:00:00Z'),
    makeRelease(fixture, 'v0.3.0', '2026-06-29T00:00:00Z'),
    makeRelease(fixture, 'v0.2.9', '2026-06-23T00:00:00Z'),
  ];
  calls = [];
});

afterEach(async () => {
  await rm(root, { recursive: true, force: true });
});

test('首轮下载镜像范围内的安装包，写出两份清单且更新器地址指向镜像', async () => {
  const result = await syncReleases(options());
  assert.equal(result.changed, true);
  assert.deepEqual((await readdir(root)).sort(), [
    'latest.json',
    'releases.json',
    'v0.3.0',
    'v0.3.1',
  ]);
  assert.equal(
    await readFile(join(root, 'v0.3.1', 'FlairBloom_0.3.1_x64-setup.exe'), 'utf8'),
    'exe v0.3.1',
  );

  const updater = JSON.parse(await readFile(join(root, 'latest.json'), 'utf8'));
  assert.equal(
    updater.platforms['windows-x86_64-nsis'].url,
    `${BASE}/v0.3.1/FlairBloom_0.3.1_x64-setup.exe`,
  );
  assert.equal(updater.platforms['windows-x86_64-nsis'].signature, 's2');

  const manifest = JSON.parse(await readFile(join(root, 'releases.json'), 'utf8'));
  assert.equal(manifest.latest, 'v0.3.1');
  assert.equal(manifest.releases.length, 3);
  assert.equal(manifest.releases[2].assets.length, 0);
});

test('第二轮无变化时不重复下载、不改写清单', async () => {
  await syncReleases(options());
  calls.length = 0;
  const result = await syncReleases(options());
  assert.equal(result.changed, false);
  assert.deepEqual(result.downloaded, []);
  assert.ok(calls.every((url) => url.startsWith('https://api.github.com/')));
});

test('sha256 对不上时整轮失败，不留半截文件，上一轮清单原样保留', async () => {
  await syncReleases(options());
  const before = await readFile(join(root, 'latest.json'), 'utf8');

  fixture.releases.unshift(makeRelease(fixture, 'v0.3.2', '2026-09-20T00:00:00Z'));
  fixture.latest = 'v0.3.2';
  fixture.corrupt = new Set([
    'https://github.com/x-wink/flair-bloom/releases/download/v0.3.2/FlairBloom_0.3.2_x64_en-US.msi',
  ]);
  await assert.rejects(syncReleases(options()), /校验失败/);

  assert.equal(await readFile(join(root, 'latest.json'), 'utf8'), before);
  assert.equal(JSON.parse(await readFile(join(root, 'releases.json'), 'utf8')).latest, 'v0.3.1');
  assert.equal(existsSync(join(root, 'v0.3.2', 'FlairBloom_0.3.2_x64_en-US.msi.part')), false);
  assert.equal(existsSync(join(root, 'v0.3.0', 'FlairBloom_0.3.0_x64-setup.exe')), true);
});

test('GitHub API 失败时不动任何已有数据', async () => {
  await syncReleases(options());
  const before = await readFile(join(root, 'releases.json'), 'utf8');
  fixture.failApi = true;
  await assert.rejects(syncReleases(options()), /403/);
  assert.equal(await readFile(join(root, 'releases.json'), 'utf8'), before);
  assert.deepEqual((await readdir(root)).sort(), [
    'latest.json',
    'releases.json',
    'v0.3.0',
    'v0.3.1',
  ]);
});

test('坏版本转草稿并把 Latest 拨回后，清单回退、坏版本目录被清理', async () => {
  fixture.releases.unshift(makeRelease(fixture, 'v0.3.2', '2026-09-20T00:00:00Z'));
  fixture.latest = 'v0.3.2';
  await syncReleases(options());
  assert.ok(existsSync(join(root, 'v0.3.2')));

  fixture.releases[0].draft = true;
  fixture.latest = 'v0.3.1';
  const result = await syncReleases(options());
  assert.equal(result.latest, 'v0.3.1');
  assert.deepEqual(result.pruned, ['v0.3.2']);
  const updater = JSON.parse(await readFile(join(root, 'latest.json'), 'utf8'));
  assert.equal(updater.version, '0.3.1');
});
