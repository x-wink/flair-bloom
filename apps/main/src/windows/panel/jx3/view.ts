// 剑网三专栏界面的纯计算：默认选哪个门派、门派与流派分组、按心法取条目、数据来源的说明文字。
import { SECT_PRESETS } from '../theme.ts';
import { type Authorization, type Column, isAuthorized, type School } from './column.ts';

export type ColumnSource = 'remote' | 'cache' | 'bundled';
export type Fallback = 'offline' | 'newer_schema' | 'invalid';

/** 后端 `get_jx3_column` 的返回；`null` 是 serde `Option` 的序列化形态 */
export interface ColumnResult {
  data: Column;
  source: ColumnSource;
  fetched_at: number | null;
  fallback: Fallback | null;
}

/** 当前主题色对应的门派或流派，对不上就选第一个门派 */
export function defaultSchoolId(column: Column, themeColor: string): string | undefined {
  const preset = SECT_PRESETS.find((p) => p.color.toLowerCase() === themeColor.toLowerCase());
  const matched = column.schools.find((s) => s.id === preset?.id);
  return (matched ?? column.schools.find((s) => s.kind === 'sect') ?? column.schools[0])?.id;
}

/** 选择器里门派在前、流派单独成组 */
export function schoolGroups(column: Column): { sects: School[]; genres: School[] } {
  return {
    sects: column.schools.filter((s) => s.kind === 'sect'),
    genres: column.schools.filter((s) => s.kind === 'genre'),
  };
}

/** 门派与流派的代表色，取主题预设 */
export function schoolColor(id: string): string | undefined {
  return SECT_PRESETS.find((p) => p.id === id)?.color;
}

const SITE_NAMES: Record<string, string> = {
  'jx3box.com': 'JX3Box',
  'nga.cn': 'NGA',
  'ngabbs.com': 'NGA',
  'bilibili.com': 'B站',
};

/** 链接所在站点的叫法：认得的写站名，其余写域名 */
export function sourceSite(url: string): string {
  let host: string;
  try {
    host = new URL(url).hostname;
  } catch {
    return '原站';
  }
  const known = Object.keys(SITE_NAMES).find((d) => host === d || host.endsWith(`.${d}`));
  return known ? SITE_NAMES[known] : host.replace(/^www\./, '');
}

/** 授权的常驻在前，没授权的公开帖折叠在后；各自保持数据里的顺序（公开帖由采集脚本按热度排好） */
export function splitByAuthorization<T extends Authorization>(
  list: T[],
): { authorized: T[]; others: T[] } {
  return {
    authorized: list.filter(isAuthorized),
    others: list.filter((e) => !isAuthorized(e)),
  };
}

export function entriesFor<T extends { spec: string }>(list: T[], spec: string | undefined): T[] {
  return spec === undefined ? [] : list.filter((e) => e.spec === spec);
}

const day = (secs: number) => {
  const d = new Date(secs * 1000);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
};

/** 底部那一行放得下的短说明；完整原因见 sourceLine，放进悬停提示 */
export function sourceBrief(result: ColumnResult): string {
  if (result.source === 'remote') return `列表更新于 ${result.data.updated_at}`;
  if (result.fallback === 'newer_schema') return '列表格式已更新，请更新应用';
  return result.source === 'cache' && result.fetched_at !== null
    ? `未连上官网 · ${day(result.fetched_at)} 的列表`
    : `未连上官网 · 内置列表（${result.data.updated_at}）`;
}

/** 数据从哪来、什么时候的；没用上线上数据时说明原因 */
export function sourceLine(result: ColumnResult): string {
  const listed = `列表更新于 ${result.data.updated_at}`;
  if (result.source === 'remote') return listed;
  const reason =
    result.fallback === 'newer_schema'
      ? '官网的列表格式已更新，请更新应用后查看最新内容'
      : result.fallback === 'invalid'
        ? '官网的列表暂时读不了'
        : '暂时连不上官网';
  const shown =
    result.source === 'cache' && result.fetched_at !== null
      ? `显示 ${day(result.fetched_at)} 取到的列表`
      : '显示安装包内置的列表';
  return `${reason}，${shown}（${listed}）`;
}
