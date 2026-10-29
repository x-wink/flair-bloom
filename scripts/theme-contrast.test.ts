// 主题对比度：21 个门派与流派预设 × 亮 / 暗，文字四档、主色文字、实底前景都要过 WCAG AA 4.5:1。
// 调色时把灰字再调淡一点、把面板淡染再调浓一点都不会有任何可见报错，所以钉在这里。
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import {
  composite,
  contrastRatio,
  ensureContrast,
  minContrast,
  parseHex,
  pickForeground,
  shiftLightness,
  toHex,
} from '../apps/main/src/windows/panel/contrast.ts';
import {
  APP_TINT_MAX,
  deriveThemeVariables,
  MODE_SURFACES,
  SECT_PRESETS,
  textBackgrounds,
} from '../apps/main/src/windows/panel/theme.ts';

const css = readFileSync(
  fileURLToPath(new URL('../apps/main/src/windows/panel/theme.css', import.meta.url)),
  'utf8',
);

/** 取某个选择器块里的 token 原值；同名 token 在亮暗两块里各有一份，不能全局匹配 */
function block(selector: string) {
  const start = css.indexOf(`${selector} {`);
  assert.notEqual(start, -1, `theme.css 里找不到 ${selector}`);
  const body = css.slice(start, css.indexOf('\n}', start));
  return (name: string): string | undefined =>
    new RegExp(`--${name}:\\s*([^;]+);`, 'u').exec(body)?.[1].trim();
}

const BLOCKS = { light: block(':root'), dark: block(":root[data-theme='dark']") } as const;
const TEXT_LADDER = ['on-surface', 'on-surface-variant', 'on-surface-muted', 'on-surface-faint'];

/** 暗色块只覆盖颜色，未覆盖的 token 沿用 :root */
const tokenOf = (mode: 'light' | 'dark', name: string) =>
  BLOCKS[mode](name) ?? BLOCKS.light(name);

test('theme.ts 的表面色与 theme.css 一致', () => {
  for (const mode of ['light', 'dark'] as const) {
    const names = ['surface', 'surface-soft', 'surface-muted'];
    assert.deepEqual(
      names.map((n) => toHex(parseHex(tokenOf(mode, `fb-${n}`) ?? ''))),
      MODE_SURFACES[mode],
      `${mode} 档 MODE_SURFACES 与 theme.css 不一致`,
    );
  }
});

test('面板主色淡染不超过 APP_TINT_MAX', () => {
  const tints = [...css.matchAll(/--fb-app-tint-[\w-]+:\s*([\d.]+);/gu)];
  assert.ok(tints.length >= 5, '没找到 --fb-app-tint-* token');
  for (const [decl, alpha] of tints) {
    assert.ok(Number(alpha) <= APP_TINT_MAX, `${decl} 超过上限 ${APP_TINT_MAX}`);
  }
  // 渐变里的主色 alpha 必须走 tint token，写死的数值绕过了上面的检查
  const bg = css.slice(css.indexOf('--fb-app-bg:'), css.indexOf(';', css.indexOf('--fb-app-bg:')));
  assert.doesNotMatch(bg, /primary-rgb\),\s*0\./u, '--fb-app-bg 写死了主色 alpha');
});

test('文字四档与语义色文字在所有预设的面板底上都过 4.5:1，四档由重到轻单调', () => {
  for (const mode of ['light', 'dark'] as const) {
    const ladder = TEXT_LADDER.map((n) => {
      const value = BLOCKS[mode](`fb-${n}`);
      assert.ok(value, `${mode} 档缺 --fb-${n}`);
      return value;
    });
    for (const preset of SECT_PRESETS) {
      const bgs = textBackgrounds(preset.color, mode);
      ladder.forEach((fg, i) => {
        const ratio = minContrast(fg, bgs);
        assert.ok(
          ratio >= 4.5,
          `${mode} ${preset.name} 上 --fb-${TEXT_LADDER[i]} ${fg} 只有 ${ratio.toFixed(2)}:1`,
        );
      });
    }
    for (const tone of ['success', 'danger', 'warning']) {
      const fg = BLOCKS[mode](`fb-color-${tone}-text`);
      assert.ok(fg, `${mode} 档缺 --fb-color-${tone}-text`);
      for (const preset of SECT_PRESETS) {
        const ratio = minContrast(fg, textBackgrounds(preset.color, mode));
        assert.ok(ratio >= 4.5, `${mode} ${preset.name} 上 ${tone} 文字 ${fg} 只有 ${ratio.toFixed(2)}:1`);
      }
    }
    const ratios = ladder.map((fg) => contrastRatio(fg, MODE_SURFACES[mode][0]));
    for (let i = 1; i < ratios.length; i += 1) {
      assert.ok(ratios[i] < ratios[i - 1], `${mode} 档文字档位不单调：${ratios.join(' > ')}`);
    }
  }
});

test('预设主色原样保留，实底前景在 normal / hover / active 三态都过 4.5:1', () => {
  for (const mode of ['light', 'dark'] as const) {
    for (const preset of SECT_PRESETS) {
      const v = deriveThemeVariables(preset.color, mode);
      assert.equal(v['--fb-color-primary'], preset.color, `${preset.name} 的主色被改动了`);
      for (const key of ['--fb-color-primary', '--fb-color-primary-hover', '--fb-color-primary-active']) {
        const ratio = contrastRatio(v['--fb-on-primary'], v[key]);
        assert.ok(ratio >= 4.5, `${mode} ${preset.name} ${key} 上前景只有 ${ratio.toFixed(2)}:1`);
      }
    }
  }
});

test('主色文字在面板底、卡片与主色淡底上都过 4.5:1', () => {
  for (const mode of ['light', 'dark'] as const) {
    for (const preset of SECT_PRESETS) {
      const text = deriveThemeVariables(preset.color, mode)['--fb-color-primary-text'];
      const bgs = textBackgrounds(preset.color, mode);
      bgs.push(composite(preset.color, bgs[bgs.length - 1], 0.18));
      const ratio = minContrast(text, bgs);
      assert.ok(ratio >= 4.5, `${mode} ${preset.name} 主色文字 ${text} 只有 ${ratio.toFixed(2)}:1`);
    }
  }
});

test('白与近黑都不过线的自定义中间调，实底沿离开前景的方向调到过线', () => {
  const mid = '#7a7a7a';
  assert.ok(minContrast(pickForeground([mid], ['#ffffff', '#111827']), [mid]) < 4.5);
  const v = deriveThemeVariables(mid, 'light');
  assert.ok(contrastRatio(v['--fb-on-primary'], v['--fb-color-primary']) >= 4.5);
});

test('shiftLightness 只动明度：加深变暗、提亮变亮，零位移不变', () => {
  const base = '#e8589b';
  assert.equal(shiftLightness(base, 0), base);
  assert.ok(contrastRatio(shiftLightness(base, -0.2), '#ffffff') > contrastRatio(base, '#ffffff'));
  assert.ok(contrastRatio(shiftLightness(base, 0.2), '#000000') > contrastRatio(base, '#000000'));
});

test('ensureContrast 已达标时原样返回', () => {
  assert.equal(ensureContrast('#1f2937', ['#ffffff'], -1), '#1f2937');
});
