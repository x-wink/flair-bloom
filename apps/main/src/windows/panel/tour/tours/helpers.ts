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
 * 实操判定的基线取「当前页签里的最后一条规则」：刚添加的那条总在末尾，而 `rule-latest`
 * 锚点也是按页签各取末尾的那张卡。取全部规则的末尾会在两边指到不同的卡——已有
 * `[hold1, toggle1]` 的用户在 hold 页签，高亮的是 hold1，判定却盯着 toggle1。
 */
export function lastRule(snapshot: TourSnapshot): TourRule | undefined {
  const inTab = snapshot.rules.filter((rule) => rule.mode === snapshot.activeTab);
  return inTab[inTab.length - 1];
}
