import { DEFAULT_THEME_COLOR, SECT_PRESETS } from '../../../main/src/windows/panel/theme.ts';

// 色板与默认色直接取应用的 theme.ts：网站与应用的门派配色必须同一份色值
export const brandPresets = SECT_PRESETS;
export const defaultBrandColor = DEFAULT_THEME_COLOR;
export const BRAND_STORAGE_KEY = 'flair-bloom:theme-color';

function darken(hex: string, factor: number): string {
  const value = hex.replace('#', '');
  return `#${[0, 2, 4]
    .map((offset) =>
      Math.round(parseInt(value.slice(offset, offset + 2), 16) * (1 - factor))
        .toString(16)
        .padStart(2, '0'),
    )
    .join('')}`;
}

export function isBrandPreset(color: string | null | undefined): color is string {
  return brandPresets.some((preset) => preset.color === color);
}

/** 与应用一致：主色明暗共用，彩色实底上的文字统一白色，深一档用作渐变与强调 */
export function brandVariables(color: string): Record<string, string> {
  return {
    '--ui-primary': color,
    '--ui-primary-fg': '#ffffff',
    '--ui-secondary': darken(color, 0.12),
    '--ui-secondary-fg': '#ffffff',
  };
}
