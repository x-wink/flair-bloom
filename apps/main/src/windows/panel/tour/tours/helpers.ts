import { Fragment, createElement, type ReactNode } from 'react';
import type { TourRule, TourSnapshot } from '../types';

/**
 * 气泡正文分段。教程文件是 `.ts` 写不了 JSX，用 createElement 包一层，
 * 免得为了几个 `<p>` 把六个文件改成 `.tsx` 牵动校验脚本与钩子的匹配。
 */
export function p(...lines: string[]): ReactNode {
  return createElement(
    Fragment,
    null,
    lines.map((line, i) => createElement('p', { key: i }, line)),
  );
}

/** 实操判定的基线取「最后一条规则」：刚添加的那条总在末尾。 */
export function lastRule(snapshot: TourSnapshot): TourRule | undefined {
  return snapshot.rules[snapshot.rules.length - 1];
}
