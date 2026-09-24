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

/** 互斥组教程的示例组：组名与三条规则的固定 id，宿主建组、教程判定共用这一份。 */
export const SAMPLE_GROUP = '示例·多段宏';
export const SAMPLE_IDS = {
  a: 'sample-macro-a',
  b: 'sample-macro-b',
  hold: 'sample-hold',
} as const;

export interface SampleRules {
  a: TourRule;
  b: TourRule;
  hold: TourRule;
}

/**
 * 示例组三条都在才算有示例组——宿主建组与教程判定共用这一个口径。只剩部分（用户删了一条、
 * 拖出组）视为脏数据，宿主重建时先清掉；第 4～6 步在不完整时直接跳过。
 */
export function isSampleRule(rule: Pick<TourRule, 'id'>): boolean {
  // 只认固定 id：用户自建的同名组、拖进示例组的自己的规则都不是示例规则，删示例时不能带走
  return (Object.values(SAMPLE_IDS) as string[]).includes(rule.id);
}

export function sampleRules(snapshot: Pick<TourSnapshot, 'rules'>): SampleRules | undefined {
  const find = (id: string) => snapshot.rules.find((r) => r.id === id);
  const a = find(SAMPLE_IDS.a);
  const b = find(SAMPLE_IDS.b);
  const hold = find(SAMPLE_IDS.hold);
  return a && b && hold ? { a, b, hold } : undefined;
}
