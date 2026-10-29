import { brandVariables as uiBrandVariables } from '@xwink/ui/brand';
import { DEFAULT_THEME_COLOR, SECT_PRESETS } from '../../../main/src/windows/panel/theme.ts';

// 色板与默认色直接取应用的 theme.ts：网站与应用的门派配色必须同一份色值
export const brandPresets = SECT_PRESETS;
export const defaultBrandColor = DEFAULT_THEME_COLOR;
export const BRAND_STORAGE_KEY = 'flair-bloom:theme-color';

export function isBrandPreset(color: string | null | undefined): color is string {
  return brandPresets.some((preset) => preset.color === color);
}

/** 与应用一致：主色明暗共用，实底文字按对比度在白与近黑里取高的，绯粉、流金等亮色用深字 */
export function brandVariables(color: string): Record<string, string> {
  return uiBrandVariables(color);
}
