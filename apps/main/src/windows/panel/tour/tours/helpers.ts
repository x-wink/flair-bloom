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

/**
 * 实操判定的基线取「当前筛选下最后一条可见规则」，与 `rule-latest` 锚点同口径：刚添加的那条
 * 总在末尾。若取全部规则的末尾，筛选时两边会指到不同的卡——高亮的是可见的那张，判定却盯着
 * 被筛掉的那条。
 */
export function lastRule(snapshot: TourSnapshot): TourRule | undefined {
  const { filter } = snapshot;
  const visible =
    filter === 'all' ? snapshot.rules : snapshot.rules.filter((rule) => rule.mode === filter);
  return visible[visible.length - 1];
}
