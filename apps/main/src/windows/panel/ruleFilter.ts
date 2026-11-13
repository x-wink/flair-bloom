export type RuleFilter = 'all' | 'hold' | 'toggle' | 'groups';

export function matchesRuleFilter(
  rule: { mode: 'hold' | 'toggle'; group?: string | null },
  filter: RuleFilter,
): boolean {
  return filter === 'all' || (filter === 'groups' ? !!rule.group : rule.mode === filter);
}
