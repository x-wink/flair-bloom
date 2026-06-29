// 主题：剑网三门派心法预设主色 + 亮/暗/跟随系统模式。
// 衍生色（hover/active/前景）在 JS 侧从主色推导后注入 CSS 变量——WebView2 不支持 color-mix，
// 透明色调统一走 --fb-color-primary-rgb 通道（见 theme.css），故只需注入少数几个变量即可全局生效。

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
 * 主题色预设。配色取自剑网三无界全 20 门派心法代表色（社区约定，非官方 hex；
 * 金/褐/黑等由 onAccent 自动配深色字）。
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

interface Rgb {
  r: number;
  g: number;
  b: number;
}

function hexToRgb(hex: string): Rgb {
  const h = hex.replace('#', '');
  const full =
    h.length === 3
      ? h
          .split('')
          .map((c) => c + c)
          .join('')
      : h;
  return {
    r: parseInt(full.slice(0, 2), 16),
    g: parseInt(full.slice(2, 4), 16),
    b: parseInt(full.slice(4, 6), 16),
  };
}

function clamp255(n: number): number {
  return Math.max(0, Math.min(255, Math.round(n)));
}

function rgbToHex({ r, g, b }: Rgb): string {
  const hex = (n: number) => clamp255(n).toString(16).padStart(2, '0');
  return `#${hex(r)}${hex(g)}${hex(b)}`;
}

/** 按比例加深（factor 0~1，越大越暗），用于 hover/active。 */
function darken(rgb: Rgb, factor: number): Rgb {
  return { r: rgb.r * (1 - factor), g: rgb.g * (1 - factor), b: rgb.b * (1 - factor) };
}

/** WCAG 相对亮度。 */
function relLuminance({ r, g, b }: Rgb): number {
  const lin = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

function contrast(l1: number, l2: number): number {
  const [hi, lo] = l1 >= l2 ? [l1, l2] : [l2, l1];
  return (hi + 0.05) / (lo + 0.05);
}

/** 在彩色实底上选对比度更高的前景（白 或 近黑）。 */
function onAccentFor(rgb: Rgb): string {
  const bg = relLuminance(rgb);
  const white = relLuminance({ r: 255, g: 255, b: 255 });
  const dark = relLuminance({ r: 45, g: 45, b: 45 }); // --fb-text-strong #2d2d2d
  return contrast(bg, white) >= contrast(bg, dark) ? '#fff' : '#2d2d2d';
}

/** 注入主色及其衍生到根元素。soft/soft-hover 透明色调通过 -rgb 通道自动跟随。 */
export function applyThemeColor(hex: string): void {
  const rgb = hexToRgb(hex);
  const root = document.documentElement.style;
  root.setProperty('--fb-color-primary', hex);
  root.setProperty('--fb-color-primary-rgb', `${rgb.r}, ${rgb.g}, ${rgb.b}`);
  root.setProperty('--fb-color-primary-hover', rgbToHex(darken(rgb, 0.12)));
  root.setProperty('--fb-color-primary-active', rgbToHex(darken(rgb, 0.22)));
  root.setProperty('--fb-text-on-accent', onAccentFor(rgb));
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
  document.documentElement.dataset.theme = resolveMode(mode);
}
