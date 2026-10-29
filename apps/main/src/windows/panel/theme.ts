// 主题：剑网三门派心法预设主色 + 亮/暗/跟随系统模式。
// 衍生色（hover/active/前景/主色文字）在 JS 侧从主色推导后注入 CSS 变量——WebView2 不支持 color-mix，
// 透明色调统一走 --fb-color-primary-rgb 通道（见 theme.css），故只需注入少数几个变量即可全局生效。
// 对比度规范同 @xwink/ui：文字在所有会落到的底色上过 WCAG AA 4.5:1，由 scripts/theme-contrast.test.ts 钉住。

import { composite, ensureContrast, parseHex, pickForeground, shiftLightness } from './contrast.ts';

export type ThemeMode = 'light' | 'dark' | 'system';

export interface ThemeSettings {
  /** 主色 hex（剑网三门派心法预设之一或自定义） */
  color: string;
  /** 亮 / 暗 / 跟随系统 */
  mode: ThemeMode;
}

export interface SectPreset {
  /** 稳定 id（持久化用），同时是门派拼音 */
  id: string;
  /** 门派名 */
  sect: string;
  /** 心法配色雅称 */
  name: string;
  /** 主色 hex */
  color: string;
}

/**
 * 主题色预设。配色取自剑网三无界全 20 门派心法代表色（社区约定，非官方 hex）。
 * 实底上的文字按对比度取白或近黑（见 deriveThemeVariables），偏亮的绯粉、流金等用深字。
 *
 * `sect` 门派名仅作来源对照、不在 UI 显示——界面只展示色块与雅称（`name`），
 * 悬停显示 `name`。下表即门派 ↔ 配色对照：
 * 万花霞紫 / 七秀绯粉 / 少林缁黄 / 纯阳天青 / 天策赤焰 / 藏剑流金 / 五毒瘴紫 /
 * 唐门玄靛 / 明教圣火 / 丐帮醉褐 / 苍云铁衣 / 长歌空青 / 霸刀沧海 / 蓬莱缥碧 /
 * 凌雪阁墨血 / 衍天宗玄阴 / 北天药宗药绿 / 万灵山庄苍木 / 刀宗赤铜 / 段氏碧玉。
 */
export const SECT_PRESETS: SectPreset[] = [
  { id: 'wanhua', sect: '万花', name: '霞紫', color: '#6c4de6' },
  { id: 'qixiu', sect: '七秀', name: '绯粉', color: '#e8589b' },
  { id: 'shaolin', sect: '少林', name: '缁黄', color: '#c98a2b' },
  { id: 'chunyang', sect: '纯阳', name: '天青', color: '#2f86c9' },
  { id: 'tiance', sect: '天策', name: '赤焰', color: '#cf3a3a' },
  { id: 'cangjian', sect: '藏剑', name: '流金', color: '#d4a017' },
  { id: 'wudu', sect: '五毒', name: '瘴紫', color: '#8e44ad' },
  { id: 'tangmen', sect: '唐门', name: '玄靛', color: '#34499c' },
  { id: 'mingjiao', sect: '明教', name: '圣火', color: '#e2622c' },
  { id: 'gaibang', sect: '丐帮', name: '醉褐', color: '#8a6a3b' },
  { id: 'cangyun', sect: '苍云', name: '铁衣', color: '#4a5260' },
  { id: 'changge', sect: '长歌', name: '空青', color: '#119b94' },
  { id: 'badao', sect: '霸刀', name: '沧海', color: '#3d6e8e' },
  { id: 'penglai', sect: '蓬莱', name: '缥碧', color: '#4aa6c0' },
  { id: 'lingxue', sect: '凌雪阁', name: '墨血', color: '#9e2b3a' },
  { id: 'yantian', sect: '衍天宗', name: '玄阴', color: '#5d5fa3' },
  { id: 'yaozong', sect: '北天药宗', name: '药绿', color: '#4f9d4f' },
  { id: 'wanling', sect: '万灵山庄', name: '苍木', color: '#6b8e3d' },
  { id: 'daozong', sect: '刀宗', name: '赤铜', color: '#b5563a' },
  { id: 'duanshi', sect: '段氏', name: '碧玉', color: '#0f9f86' },
];

export const DEFAULT_THEME_COLOR = SECT_PRESETS[0].color;
export const DEFAULT_THEME_MODE: ThemeMode = 'light';

/**
 * 面板背景主色淡染（theme.css 的 --fb-app-tint-*）允许的最大 alpha。文字四档与主色文字都按
 * 「叠到这个浓度的底色」验 4.5:1，最弱的 faint 档在 0.16 左右到顶，留一点余量。
 */
export const APP_TINT_MAX = 0.15;

/** 与 theme.css 的 --fb-surface / -soft / -muted 同值，scripts/theme-contrast.test.ts 核对两边一致 */
export const MODE_SURFACES: Record<'light' | 'dark', readonly string[]> = {
  light: ['#ffffff', '#fafafa', '#f4f4f6'],
  dark: ['#202126', '#26272c', '#2c2d33'],
};

/** 暗色卡片的 --fb-surface-translucent（白 0.06）叠在淡染底上，会把底色再提亮一点 */
const DARK_CARD_OVERLAY = 0.06;

/** 主色淡底 hover（--fb-color-primary-soft-hover）的 alpha，主色文字常落在上面 */
const PRIMARY_SOFT_MAX = 0.18;

/** 实底前景候选：白与近黑（同 @xwink/ui brandVariables 的缺省候选） */
export const ON_PRIMARY_CANDIDATES = ['#ffffff', '#111827'] as const;

/** hover / active 沿 OKLCH 明度离开前景色方向各走一档 */
const SHADE_STEP = 0.06;

/** 文字会落在的所有底色：纯表面、叠满淡染的面板底，暗色再加卡片半透明层 */
export function textBackgrounds(primary: string, mode: 'light' | 'dark'): string[] {
  const surfaces = MODE_SURFACES[mode];
  const tinted = composite(primary, surfaces[0], APP_TINT_MAX);
  const bgs = [...surfaces, tinted];
  if (mode === 'dark') bgs.push(composite('#ffffff', tinted, DARK_CARD_OVERLAY));
  return bgs;
}

/**
 * 从主色与生效明暗推导注入的变量：
 * - 实底前景按对比度在白与近黑里取高的；两者都不到 4.5（自定义的中间调）时把实底沿离开前景的方向调到过线，
 *   预设色都不会走到这一步，由测试钉住。
 * - hover / active 往离开前景的方向走：白字的加深、深字的提亮，对比度只增不减。
 * - 主色当文字用（链接、选中页签、描边按钮）时取 --fb-color-primary-text：亮色档加深、暗色档提亮，
 *   直到在面板底、卡片、主色淡底上都过 4.5。
 */
export function deriveThemeVariables(
  color: string,
  mode: 'light' | 'dark',
): Record<string, string> {
  const onPrimary = pickForeground([color], ON_PRIMARY_CANDIDATES);
  const away = onPrimary === '#ffffff' ? -1 : 1;
  const solid = ensureContrast(color, [onPrimary], away);
  const rgb = parseHex(solid);

  const backgrounds = textBackgrounds(solid, mode);
  backgrounds.push(composite(solid, backgrounds[backgrounds.length - 1], PRIMARY_SOFT_MAX));
  const primaryText = ensureContrast(solid, backgrounds, mode === 'light' ? -1 : 1);

  return {
    '--fb-color-primary': solid,
    '--fb-color-primary-rgb': `${rgb.r}, ${rgb.g}, ${rgb.b}`,
    '--fb-color-primary-hover': shiftLightness(solid, away * SHADE_STEP),
    '--fb-color-primary-active': shiftLightness(solid, away * SHADE_STEP * 2),
    '--fb-color-primary-text': primaryText,
    '--fb-on-primary': onPrimary,
  };
}

let currentColor = DEFAULT_THEME_COLOR;
let currentMode: ThemeMode = DEFAULT_THEME_MODE;

function applyTheme(): void {
  const resolved = resolveMode(currentMode);
  const root = document.documentElement;
  root.dataset.theme = resolved;
  for (const [name, value] of Object.entries(deriveThemeVariables(currentColor, resolved))) {
    root.style.setProperty(name, value);
  }
}

/** 换主色。主色文字随明暗不同，所以与 applyThemeMode 共用一次完整推导。 */
export function applyThemeColor(hex: string): void {
  currentColor = hex;
  applyTheme();
}

/** 解析 system → 实际亮暗。 */
function resolveMode(mode: ThemeMode): 'light' | 'dark' {
  if (mode === 'system') {
    return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }
  return mode;
}

/** 设置 data-theme（light/dark），由 theme.css 的 :root[data-theme="dark"] 接管暗色 token。 */
export function applyThemeMode(mode: ThemeMode): void {
  currentMode = mode;
  applyTheme();
}
