export interface GithubAsset {
  name: string;
  size: number;
  digest: string | null;
  browser_download_url: string;
}

export interface GithubRelease {
  tag_name: string;
  name: string | null;
  body: string | null;
  draft: boolean;
  prerelease: boolean;
  published_at: string | null;
  html_url: string;
  assets: GithubAsset[];
}

export type InstallerKind = 'nsis' | 'msi';

export interface MirroredAsset {
  kind: InstallerKind;
  name: string;
  size: number;
  sha256: string;
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
  /** 只有镜像范围内的版本才有；更早的版本页面直接给 GitHub 链接 */
  assets: MirroredAsset[];
}

export interface ReleasesManifest {
  schema: 1;
  generatedAt: string;
  latest: string;
  releases: ReleaseEntry[];
}

export interface DownloadItem {
  tag: string;
  name: string;
  size: number;
  sha256: string;
  githubUrl: string;
}

export interface MirrorPlan {
  latest: GithubRelease;
  /** 公告保留的版本，按发布时间倒序 */
  noted: GithubRelease[];
  /** 镜像安装包的版本：Latest 指针所指版本必在其中，即使它不是最新发布的 */
  mirrored: GithubRelease[];
  downloads: DownloadItem[];
  latestJson: DownloadItem;
}

export const LATEST_JSON = 'latest.json';

// tag 与资产名会拼进服务器上的文件路径，只放行 GitHub 实际会产出的字符，挡住 ../ 这类穿越
const SAFE_SEGMENT = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;

export function assertSafeSegment(value: string): string {
  if (!SAFE_SEGMENT.test(value) || value.includes('..')) {
    throw new Error(`不安全的路径片段：${value}`);
  }
  return value;
}

export function installerKind(name: string): InstallerKind | undefined {
  if (name.endsWith('-setup.exe')) return 'nsis';
  if (name.endsWith('.msi')) return 'msi';
  return undefined;
}

export function sha256Of(asset: GithubAsset): string {
  const hex = /^sha256:([0-9a-f]{64})$/.exec(asset.digest ?? '')?.[1];
  if (!hex) throw new Error(`资产 ${asset.name} 缺少 sha256 digest`);
  return hex;
}

export function stableReleases(releases: GithubRelease[]): GithubRelease[] {
  return releases
    .filter((release) => !release.draft && !release.prerelease && release.published_at)
    .sort((left, right) => (right.published_at ?? '').localeCompare(left.published_at ?? ''));
}

function downloadItem(release: GithubRelease, asset: GithubAsset): DownloadItem {
  return {
    tag: assertSafeSegment(release.tag_name),
    name: assertSafeSegment(asset.name),
    size: asset.size,
    sha256: sha256Of(asset),
    githubUrl: asset.browser_download_url,
  };
}

export function planMirror(
  releases: GithubRelease[],
  latestTag: string,
  options: { keepNotes: number; keepAssets: number },
): MirrorPlan {
  const stable = stableReleases(releases);
  const latest = stable.find((release) => release.tag_name === latestTag);
  // Latest 指针指向草稿或预发布不是正常状态，宁可整轮失败保留旧数据，也不发布一份自相矛盾的清单
  if (!latest) throw new Error(`Latest 指针 ${latestTag} 不在稳定版本列表里`);

  const noted = stable.slice(0, options.keepNotes);
  if (!noted.includes(latest)) noted.push(latest);

  const mirrored = stable.slice(0, options.keepAssets);
  if (!mirrored.includes(latest)) mirrored.push(latest);

  const downloads = mirrored.flatMap((release) =>
    release.assets
      .filter((asset) => installerKind(asset.name))
      .map((asset) => downloadItem(release, asset)),
  );
  const latestJsonAsset = latest.assets.find((asset) => asset.name === LATEST_JSON);
  if (!latestJsonAsset) throw new Error(`${latest.tag_name} 缺少 ${LATEST_JSON}`);

  return { latest, noted, mirrored, downloads, latestJson: downloadItem(latest, latestJsonAsset) };
}

export function mirrorUrl(publicBaseUrl: string, tag: string, name: string): string {
  return `${publicBaseUrl.replace(/\/+$/, '')}/${encodeURIComponent(tag)}/${encodeURIComponent(name)}`;
}

export function buildManifest(
  plan: MirrorPlan,
  publicBaseUrl: string,
  now: Date,
): ReleasesManifest {
  const mirroredTags = new Set(plan.mirrored.map((release) => release.tag_name));
  const noted = [...plan.noted].sort((left, right) =>
    (right.published_at ?? '').localeCompare(left.published_at ?? ''),
  );
  return {
    schema: 1,
    generatedAt: now.toISOString(),
    latest: plan.latest.tag_name,
    releases: noted.map((release) => ({
      tag: release.tag_name,
      version: release.tag_name.replace(/^v/, ''),
      name: release.name ?? release.tag_name,
      publishedAt: release.published_at ?? '',
      notes: release.body ?? '',
      htmlUrl: release.html_url,
      assets: mirroredTags.has(release.tag_name)
        ? release.assets.flatMap((asset) => {
            const kind = installerKind(asset.name);
            if (!kind) return [];
            return [
              {
                kind,
                name: asset.name,
                size: asset.size,
                sha256: sha256Of(asset),
                url: mirrorUrl(publicBaseUrl, release.tag_name, asset.name),
                githubUrl: asset.browser_download_url,
              },
            ];
          })
        : [],
    })),
  };
}

interface UpdaterManifest {
  version: string;
  platforms: Record<string, { url: string; signature: string }>;
}

/**
 * 把更新器清单里的下载地址换成镜像地址。签名只覆盖安装包内容、不覆盖 URL，改写不影响校验；
 * 但只要有一个平台的地址不在镜像里就整体失败——半改写的清单会让部分用户走到不存在的文件。
 */
export function rewriteLatestJson(raw: string, urlMap: ReadonlyMap<string, string>): string {
  const manifest = JSON.parse(raw) as UpdaterManifest;
  if (!manifest.platforms || typeof manifest.platforms !== 'object') {
    throw new Error(`${LATEST_JSON} 缺少 platforms`);
  }
  for (const [platform, entry] of Object.entries(manifest.platforms)) {
    const mirrored = urlMap.get(entry.url);
    if (!mirrored) throw new Error(`${LATEST_JSON} 的 ${platform} 指向未镜像的地址：${entry.url}`);
    entry.url = mirrored;
  }
  return `${JSON.stringify(manifest, undefined, 2)}\n`;
}
