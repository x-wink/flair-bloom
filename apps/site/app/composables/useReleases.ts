export const GITHUB_RELEASES_URL = `${REPOSITORY}/releases`;
/** 国内直连 GitHub 基本拉不动，下载与清单都先走这个公共加速代理 */
export const GITHUB_PROXY = 'https://gh-proxy.com/';

const RELEASES_API = 'https://api.github.com/repos/x-wink/flair-bloom/releases?per_page=10';

export type InstallerKind = 'nsis' | 'msi';

export interface ReleaseAsset {
  kind: InstallerKind;
  name: string;
  size: number;
  /** 加速代理后的下载地址，页面上所有下载入口都用它 */
  url: string;
  githubUrl: string;
}

export interface ReleaseEntry {
  tag: string;
  version: string;
  name: string;
  publishedAt: string;
  notes: string;
  htmlUrl: string;
  assets: ReleaseAsset[];
}

interface GithubAsset {
  name: string;
  size: number;
  browser_download_url: string;
}

interface GithubRelease {
  tag_name: string;
  name: string | null;
  body: string | null;
  draft: boolean;
  prerelease: boolean;
  published_at: string | null;
  html_url: string;
  assets: GithubAsset[];
}

export function proxied(url: string): string {
  return `${GITHUB_PROXY}${url}`;
}

export function installerKind(name: string): InstallerKind | undefined {
  if (name.endsWith('-setup.exe')) return 'nsis';
  if (name.endsWith('.msi')) return 'msi';
  return undefined;
}

/** 草稿与预发布不对外；published_at 缺失的是还没真正发出来的记录 */
function toEntry(release: GithubRelease): ReleaseEntry {
  return {
    tag: release.tag_name,
    version: release.tag_name.replace(/^v/, ''),
    name: release.name ?? release.tag_name,
    publishedAt: release.published_at ?? '',
    notes: release.body ?? '',
    htmlUrl: release.html_url,
    assets: release.assets.flatMap((asset) => {
      const kind = installerKind(asset.name);
      if (!kind) return [];
      return [
        {
          kind,
          name: asset.name,
          size: asset.size,
          url: proxied(asset.browser_download_url),
          githubUrl: asset.browser_download_url,
        },
      ];
    }),
  };
}

export function toReleaseEntries(releases: GithubRelease[]): ReleaseEntry[] {
  return releases
    .filter((release) => !release.draft && !release.prerelease && release.published_at)
    .sort((left, right) => (right.published_at ?? '').localeCompare(left.published_at ?? ''))
    .map(toEntry);
}

/**
 * 代理优先、GitHub 直连兜底，与应用更新器同一套路：代理限流或抽风时至少还能读到清单。
 * 两条都失败才算失败，页面退回「前往 GitHub 发布页」。
 */
async function fetchReleases(): Promise<ReleaseEntry[]> {
  let last: unknown;
  for (const url of [proxied(RELEASES_API), RELEASES_API]) {
    try {
      const response = await fetch(url, { headers: { Accept: 'application/vnd.github+json' } });
      if (!response.ok) throw new Error(`${url} → ${response.status}`);
      return toReleaseEntries((await response.json()) as GithubRelease[]);
    } catch (error) {
      last = error;
    }
  }
  throw last instanceof Error ? last : new Error('读取发布列表失败');
}

/**
 * 发布数据只在浏览器里读：页面是 SSG，构建时把清单烤进去就得每次 app 发版都重发站点。
 * 最新版本取列表里最近发布的那个稳定版。
 */
export function useReleases() {
  const { data, status } = useAsyncData('releases', fetchReleases, { server: false, lazy: true });

  const releases = computed<ReleaseEntry[]>(() => data.value ?? []);
  const latest = computed<ReleaseEntry | undefined>(() => releases.value[0]);

  return { releases, latest, status };
}

export function formatSize(bytes: number): string {
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function formatDate(iso: string): string {
  return iso.slice(0, 10);
}
