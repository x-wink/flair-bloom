import type { ReleaseEntry, ReleasesManifest } from '../../mirror/manifest.ts';

export type { MirroredAsset, ReleaseEntry } from '../../mirror/manifest.ts';

export const GITHUB_RELEASES_URL = 'https://github.com/x-wink/flair-bloom/releases';

/**
 * 发布数据只在浏览器里读：页面是 SSG，构建时把清单烤进去就得每次 app 发版都重发站点。
 * 清单由服务器上的镜像同步维护，与更新器读的是同一批数据。
 */
export function useReleases() {
  const { data, status } = useFetch<ReleasesManifest>('releases/releases.json', {
    baseURL: useRuntimeConfig().app.baseURL,
    server: false,
    lazy: true,
  });

  const latest = computed<ReleaseEntry | undefined>(() =>
    data.value?.releases.find((release) => release.tag === data.value?.latest),
  );

  return { manifest: data, latest, status };
}

export function formatSize(bytes: number): string {
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function formatDate(iso: string): string {
  return iso.slice(0, 10);
}
