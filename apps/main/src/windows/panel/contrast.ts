// 颜色对比度与 OKLCH 明度调整的纯函数，算法与 @xwink/ui/brand 一致（WCAG 2.x 相对亮度、
// OKLCH 只动明度、出色域收色度）。没有直接依赖那个包：发版工作流不带私有源凭据，
// 且 Node 测试不剥离 node_modules 里的 TS；界面迁到 Vue 与组件库后换回包里的实现。

export interface Rgb {
  r: number;
  g: number;
  b: number;
}

export function parseHex(color: string): Rgb {
  const h = color.replace('#', '');
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

export function toHex({ r, g, b }: Rgb): string {
  const hex = (n: number) =>
    Math.round(Math.max(0, Math.min(255, n)))
      .toString(16)
      .padStart(2, '0');
  return `#${hex(r)}${hex(g)}${hex(b)}`;
}

/** 半透明前景按 alpha 叠在不透明底色上的结果，用来算渐变底、淡染底上的实际底色 */
export function composite(fg: string, bg: string, alpha: number): string {
  const f = parseHex(fg);
  const b = parseHex(bg);
  const mix = (x: number, y: number) => x * alpha + y * (1 - alpha);
  return toHex({ r: mix(f.r, b.r), g: mix(f.g, b.g), b: mix(f.b, b.b) });
}

const toLinear = (channel: number) => {
  const c = channel / 255;
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
};

const fromLinear = (c: number) =>
  255 * (c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055);

/** WCAG 2.x 相对亮度 */
export function relativeLuminance(color: string): number {
  const { r, g, b } = parseHex(color);
  return 0.2126 * toLinear(r) + 0.7152 * toLinear(g) + 0.0722 * toLinear(b);
}

/** WCAG 2.x 对比度，1–21 */
export function contrastRatio(a: string, b: string): number {
  const [light, dark] = [relativeLuminance(a), relativeLuminance(b)].sort((x, y) => y - x);
  return (light + 0.05) / (dark + 0.05);
}

/** 前景在一组底色上的最低对比度 */
export function minContrast(fg: string, backgrounds: readonly string[]): number {
  return Math.min(...backgrounds.map((bg) => contrastRatio(fg, bg)));
}

interface Oklab {
  l: number;
  a: number;
  b: number;
}

// OKLab 变换系数取自 Björn Ottosson 的原始定义（CSS Color 4 的 oklch() 同源）
function toOklab({ r, g, b }: Rgb): Oklab {
  const [lr, lg, lb] = [toLinear(r), toLinear(g), toLinear(b)];
  const l = Math.cbrt(0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb);
  const m = Math.cbrt(0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb);
  const s = Math.cbrt(0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb);
  return {
    l: 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    a: 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    b: 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  };
}

function oklabToLinear({ l: lightness, a, b }: Oklab): [number, number, number] {
  const l = (lightness + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (lightness - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (lightness - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}

const GAMUT_EPSILON = 1e-4;
const inGamut = (channels: readonly number[]) =>
  channels.every((c) => c >= -GAMUT_EPSILON && c <= 1 + GAMUT_EPSILON);

/**
 * 在 OKLCH 里只改明度（delta 负为加深、正为提亮），色相不动；出 sRGB 色域就收色度拉回，
 * 不逐通道截断——截断会把色相拧偏。不按 RGB 等比压暗：饱和色压完发灰发脏。
 */
export function shiftLightness(color: string, delta: number): string {
  const lab = toOklab(parseHex(color));
  const l = Math.min(1, Math.max(0, lab.l + delta));
  let low = 0;
  let high = 1;
  let channels = oklabToLinear({ l, a: lab.a, b: lab.b });
  if (!inGamut(channels)) {
    for (let step = 0; step < 24; step += 1) {
      const scale = (low + high) / 2;
      const trial = oklabToLinear({ l, a: lab.a * scale, b: lab.b * scale });
      if (inGamut(trial)) {
        low = scale;
        channels = trial;
      } else {
        high = scale;
      }
    }
    if (!inGamut(channels)) channels = oklabToLinear({ l, a: 0, b: 0 });
  }
  const [r, g, b] = channels.map((c) => fromLinear(Math.min(1, Math.max(0, c))));
  return toHex({ r, g, b });
}

/**
 * 沿 OKLCH 明度一步步加深（direction=-1）或提亮（+1），直到在所有底色上都达到 min。
 * 本身已达标就原样返回，保证够用的主色不被无谓改动。
 */
export function ensureContrast(
  color: string,
  backgrounds: readonly string[],
  direction: -1 | 1,
  min = 4.5,
): string {
  let current = color;
  for (let step = 1; step <= 100 && minContrast(current, backgrounds) < min; step += 1) {
    current = shiftLightness(color, direction * step * 0.01);
  }
  return current;
}

/** 在候选前景里取对每个底色最低对比度最高的一个；并列时取第一个 */
export function pickForeground(
  backgrounds: readonly string[],
  candidates: readonly string[],
): string {
  let best = candidates[0];
  let bestRatio = minContrast(best, backgrounds);
  for (const candidate of candidates.slice(1)) {
    const ratio = minContrast(candidate, backgrounds);
    if (ratio > bestRatio) {
      best = candidate;
      bestRatio = ratio;
    }
  }
  return best;
}
