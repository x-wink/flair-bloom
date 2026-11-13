import type { TourRule, TourSnapshot } from '../types';
import { matchesRuleFilter } from '../../ruleFilter.ts';

/** 排除被筛掉或折叠的规则，确保判定与高亮指向同一张卡。 */
export function lastRule(
  snapshot: Pick<TourSnapshot, 'rules' | 'filter' | 'collapsedGroups'>,
): TourRule | undefined {
  const { filter, collapsedGroups } = snapshot;
  const visible = snapshot.rules.filter(
    (rule) =>
      matchesRuleFilter(rule, filter) && !(rule.group && collapsedGroups.includes(rule.group)),
  );
  return visible[visible.length - 1];
}
