import { createHash } from 'node:crypto';
import { createReadStream, createWriteStream } from 'node:fs';
import { mkdir, readdir, readFile, rename, rm, stat, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { Readable, Transform } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { pathToFileURL } from 'node:url';
import {
  LATEST_JSON,
  buildManifest,
  mirrorUrl,
  planMirror,
  rewriteLatestJson,
  type DownloadItem,
  type GithubRelease,
} from './manifest.ts';

export const RELEASES_JSON = 'releases.json';

export interface SyncOptions {
  root: string;
  repository: string;
  publicBaseUrl: string;
  keepNotes: number;
  keepAssets: number;
  token?: string;
  /** 下载代理前缀，拼在 GitHub 下载地址前；不传就只直连 */
  downloadProxy?: string;
  fetch?: typeof fetch;
  now?: () => Date;
  log?: (message: string) => void;
}

export interface SyncResult {
  latest: string;
  downloaded: string[];
  pruned: string[];
  changed: boolean;
}

export const DEFAULT_DOWNLOAD_PROXY = 'https://gh-proxy.com/';

const API_TIMEOUT_MS = 30_000;
// 服务器经代理下载 6 MB 实测一秒内；卡住就尽快放弃，回落直连
const PROXY_TIMEOUT_MS = 5 * 60_000;
// 只在代理失败时用到：国内服务器直连 GitHub 资产实测约 20 KB/s，9 MB 的 msi 要七分钟左右
const DIRECT_TIMEOUT_MS = 30 * 60_000;

async function githubJson<T>(options: SyncOptions, path: string): Promise<T> {
  const response = await (options.fetch ?? fetch)(`https://api.github.com${path}`, {
    headers: {
      Accept: 'application/vnd.github+json',
      'User-Agent': 'flair-bloom-release-mirror',
      ...(options.token ? { Authorization: `Bearer ${options.token}` } : {}),
    },
    signal: AbortSignal.timeout(API_TIMEOUT_MS),
  });
  if (!response.ok) throw new Error(`GitHub API ${path} → ${response.status}`);
  return (await response.json()) as T;
}

async function sha256File(path: string): Promise<string | undefined> {
  try {
    const hash = createHash('sha256');
    await pipeline(createReadStream(path), hash);
    return hash.digest('hex');
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return undefined;
    throw error;
  }
}

/** 下载到 .part，大小与 sha256 都对上才改名；失败时删掉半截文件，已有的正式文件不受影响 */
async function download(
  options: SyncOptions,
  item: DownloadItem,
  dest: string,
  url: string,
  timeoutMs: number,
): Promise<void> {
  const part = `${dest}.part`;
  const response = await (options.fetch ?? fetch)(url, {
    headers: { 'User-Agent': 'flair-bloom-release-mirror' },
    redirect: 'follow',
    signal: AbortSignal.timeout(timeoutMs),
  });
  if (!response.ok || !response.body)
    throw new Error(`下载 ${item.tag}/${item.name} → ${response.status}`);

  const hash = createHash('sha256');
  let size = 0;
  const meter = new Transform({
    transform(chunk: Buffer, _encoding, callback) {
      size += chunk.length;
      hash.update(chunk);
      callback(undefined, chunk);
    },
  });
  try {
    await pipeline(Readable.fromWeb(response.body as never), meter, createWriteStream(part));
    const digest = hash.digest('hex');
    if (size !== item.size || digest !== item.sha256) {
      throw new Error(
        `${item.tag}/${item.name} 校验失败：大小 ${size}/${item.size}，sha256 ${digest}`,
      );
    }
    await rename(part, dest);
  } catch (error) {
    await rm(part, { force: true });
    throw error;
  }
}

async function ensureAsset(options: SyncOptions, item: DownloadItem): Promise<boolean> {
  const dir = join(options.root, item.tag);
  const dest = join(dir, item.name);
  if ((await sha256File(dest)) === item.sha256) return false;
  await mkdir(dir, { recursive: true });
  // 第三方代理的可信度由 sha256 兜底：内容被改过也过不了校验，只会触发回落直连，不会落盘
  if (options.downloadProxy) {
    try {
      await download(
        options,
        item,
        dest,
        `${options.downloadProxy}${item.githubUrl}`,
        PROXY_TIMEOUT_MS,
      );
      return true;
    } catch (error) {
      logOf(options)(
        `代理下载 ${item.tag}/${item.name} 失败，回落直连 GitHub：${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }
  await download(options, item, dest, item.githubUrl, DIRECT_TIMEOUT_MS);
  return true;
}

function logOf(options: SyncOptions): (message: string) => void {
  return options.log ?? ((message: string) => console.log(`[mirror] ${message}`));
}

async function readText(path: string): Promise<string | undefined> {
  try {
    return await readFile(path, 'utf8');
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return undefined;
    throw error;
  }
}

function withoutTimestamp(manifest: string | undefined): string | undefined {
  return manifest?.replace(/"generatedAt": "[^"]*"/, '');
}

export async function syncReleases(options: SyncOptions): Promise<SyncResult> {
  const log = logOf(options);
  const repo = options.repository;

  const [releases, latest] = await Promise.all([
    githubJson<GithubRelease[]>(options, `/repos/${repo}/releases?per_page=100`),
    githubJson<GithubRelease>(options, `/repos/${repo}/releases/latest`),
  ]);
  const plan = planMirror(releases, latest.tag_name, options);
  await mkdir(options.root, { recursive: true });

  const downloaded: string[] = [];
  for (const item of plan.downloads) {
    if (await ensureAsset(options, item)) downloaded.push(`${item.tag}/${item.name}`);
  }

  // 更新器清单先在内存里改写并校验，失败时磁盘上的两份清单都还是上一轮的
  const latestJsonPath = join(options.root, plan.latestJson.tag, LATEST_JSON);
  if (await ensureAsset(options, plan.latestJson))
    downloaded.push(`${plan.latestJson.tag}/${LATEST_JSON}`);
  const urlMap = new Map(
    plan.downloads.map((item) => [
      item.githubUrl,
      mirrorUrl(options.publicBaseUrl, item.tag, item.name),
    ]),
  );
  const rewritten = rewriteLatestJson(await readFile(latestJsonPath, 'utf8'), urlMap);
  const manifest = `${JSON.stringify(buildManifest(plan, options.publicBaseUrl, (options.now ?? (() => new Date()))()), undefined, 2)}\n`;

  const releasesPath = join(options.root, RELEASES_JSON);
  const updaterPath = join(options.root, LATEST_JSON);
  const changed =
    withoutTimestamp(await readText(releasesPath)) !== withoutTimestamp(manifest) ||
    (await readText(updaterPath)) !== rewritten;
  if (changed) {
    await writeFile(`${releasesPath}.tmp`, manifest);
    await writeFile(`${updaterPath}.tmp`, rewritten);
    // 先换公告清单再换更新器清单：更新器看到新版本时，下载页一定已经有它
    await rename(`${releasesPath}.tmp`, releasesPath);
    await rename(`${updaterPath}.tmp`, updaterPath);
  }

  const keep = new Set(plan.mirrored.map((release) => release.tag_name));
  const pruned: string[] = [];
  for (const entry of await readdir(options.root, { withFileTypes: true })) {
    if (!entry.isDirectory() || keep.has(entry.name)) continue;
    await rm(join(options.root, entry.name), { recursive: true, force: true });
    pruned.push(entry.name);
  }

  log(
    `latest=${plan.latest.tag_name} changed=${changed} downloaded=[${downloaded.join(', ')}] pruned=[${pruned.join(', ')}]`,
  );
  return { latest: plan.latest.tag_name, downloaded, pruned, changed };
}

function integerEnv(name: string, fallback: number): number {
  const raw = process.env[name];
  if (raw === undefined || raw === '') return fallback;
  const value = Number(raw);
  if (!Number.isInteger(value) || value < 1) throw new Error(`${name} 必须是正整数，收到：${raw}`);
  return value;
}

async function main(): Promise<void> {
  const root = process.env.MIRROR_ROOT;
  if (!root) throw new Error('缺少 MIRROR_ROOT');
  await stat(root).catch(() => mkdir(root, { recursive: true }));
  await syncReleases({
    root,
    repository: process.env.MIRROR_REPOSITORY ?? 'x-wink/flair-bloom',
    publicBaseUrl:
      process.env.MIRROR_PUBLIC_BASE_URL ?? 'https://app.xwink.fun/flair-bloom/releases',
    keepNotes: integerEnv('MIRROR_KEEP_NOTES', 30),
    keepAssets: integerEnv('MIRROR_KEEP_ASSETS', 3),
    token: process.env.GITHUB_TOKEN || undefined,
    // 未设置走默认代理；显式设为空字符串则只直连
    downloadProxy: process.env.MIRROR_DOWNLOAD_PROXY ?? DEFAULT_DOWNLOAD_PROXY,
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error: unknown) => {
    console.error(
      `[mirror] 同步失败，保留上一轮数据：${error instanceof Error ? error.message : String(error)}`,
    );
    process.exitCode = 1;
  });
}
