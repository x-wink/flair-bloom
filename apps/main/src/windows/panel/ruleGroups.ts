import type { CardRule } from './RuleCard';
import { newRule } from './ruleDefaults.ts';

export function ruleGroupNames(rules: Pick<CardRule, 'group'>[]): string[] {
  return [...new Set(rules.flatMap((rule) => (rule.group ? [rule.group] : [])))];
}

export function shouldClearRuleGroupByDefault(
  rules: Pick<CardRule, 'group' | 'enabled'>[],
  name: string,
): boolean {
  const members = rules.filter((rule) => rule.group === name);
  return members.length > 0 && members.every((rule) => !rule.enabled);
}

export function disbandRuleGroup(
  rules: CardRule[],
  name: string,
  deleteMembers = false,
): CardRule[] {
  return deleteMembers
    ? rules.filter((rule) => rule.group !== name)
    : rules.map((rule) => (rule.group === name ? { ...rule, group: null } : rule));
}

export function createRuleGroup(
  rules: CardRule[],
  moveRuleId?: string,
): { name: string; rules: CardRule[] } {
  const names = new Set(ruleGroupNames(rules));
  let index = 1;
  while (names.has(`规则组 ${index}`)) index++;
  const name = `规则组 ${index}`;
  const defaults: CardRule[] = [0x31, 0x32].map((code) => ({
    ...newRule('toggle', { kind: 'keyboard', code }),
    enabled: false,
    group: name,
  }));
  return {
    name,
    rules: [
      ...rules.map((rule) => (rule.id === moveRuleId ? { ...rule, group: name } : rule)),
      ...defaults,
    ],
  };
}
