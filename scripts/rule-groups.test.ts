import assert from 'node:assert/strict';
import test from 'node:test';
import {
  createRuleGroup,
  disbandRuleGroup,
  ruleGroupNames,
  shouldClearRuleGroupByDefault,
} from '../apps/main/src/windows/panel/ruleGroups.ts';
import { newRule } from '../apps/main/src/windows/panel/ruleDefaults.ts';

test('新规则组包含两条停用的同键切换规则，按键为主键区 1 和 2', () => {
  const result = createRuleGroup([]);
  assert.equal(result.name, '规则组 1');
  assert.deepEqual(
    result.rules.map(({ id, ...rule }) => rule),
    [49, 50].map((code) => ({
      enabled: false,
      trigger_key: { kind: 'keyboard', code },
      steps: [{ key: { kind: 'keyboard', code }, interval_ms: 10 }],
      mode: 'toggle',
      stop_key: null,
      group: '规则组 1',
    })),
  );
  assert.equal(new Set(result.rules.map((rule) => rule.id)).size, 2);
});

test('重复创建跳过已有组名并保留现有规则和自定义名称', () => {
  const first = createRuleGroup([]);
  first.rules[1].group = '用户自定义';
  const before = structuredClone(first.rules);
  const second = createRuleGroup(first.rules);
  assert.equal(second.name, '规则组 2');
  assert.deepEqual(second.rules.slice(0, 2), before);
  assert.deepEqual(first.rules, before);
  assert.equal(new Set(second.rules.map((rule) => rule.id)).size, 4);
});

test('规则菜单新建组时只移动选中规则，并保留两条默认切换规则', () => {
  const original = createRuleGroup([]).rules;
  const before = structuredClone(original);
  const result = createRuleGroup(original, original[0].id);
  assert.equal(result.name, '规则组 2');
  assert.deepEqual(result.rules[0], { ...before[0], group: result.name });
  assert.deepEqual(result.rules[1], before[1]);
  assert.equal(result.rules.length, 4);
  assert.deepEqual(original, before);
});

test('独立添加规则保留原有长按和切换默认值，并可指定同键规则', () => {
  const hold = newRule('hold');
  const toggle = newRule('toggle');
  assert.equal(hold.enabled, false);
  assert.equal(toggle.enabled, true);
  assert.deepEqual(hold.trigger_key, { kind: 'keyboard', code: 81 });
  assert.deepEqual(toggle.trigger_key, { kind: 'keyboard', code: 70 });
  assert.deepEqual(toggle.steps[0].key, hold.steps[0].key);
  const key = { kind: 'keyboard' as const, code: 49 };
  const single = newRule('toggle', key);
  assert.deepEqual(single.trigger_key, key);
  assert.deepEqual(single.steps[0].key, key);
});

test('规则组只由实际成员生成，最后一条成员移出或删除后不保留空壳', () => {
  const group = createRuleGroup([]);
  assert.deepEqual(ruleGroupNames(group.rules), ['规则组 1']);
  assert.deepEqual(ruleGroupNames(group.rules.slice(1)), ['规则组 1']);
  assert.deepEqual(ruleGroupNames([]), []);
  assert.deepEqual(ruleGroupNames(group.rules.map((rule) => ({ ...rule, group: null }))), []);
  assert.deepEqual(
    ruleGroupNames([{ group: '互斥组1' }, { group: '用户组' }, { group: '互斥组1' }]),
    ['互斥组1', '用户组'],
  );
});

test('解散时仅在目标组所有规则停用时默认清空，不受其他组或未分组规则影响', () => {
  const group = createRuleGroup([]);
  const otherGroup = { ...newRule('toggle'), group: '其他组' };
  const ungrouped = newRule('toggle');
  const rules = [...group.rules, otherGroup, ungrouped];
  const before = structuredClone(rules);
  assert.equal(shouldClearRuleGroupByDefault(rules, group.name), true);
  assert.deepEqual(rules, before);
});

test('解散时只要组内有规则启用就默认保留，与是否正在运行无关', () => {
  const group = createRuleGroup([]);
  for (let index = 0; index < group.rules.length; index++) {
    const rules = group.rules.map((rule, i) => ({ ...rule, enabled: i === index }));
    assert.equal(shouldClearRuleGroupByDefault(rules, group.name), false);
  }
  assert.equal(
    shouldClearRuleGroupByDefault(
      group.rules.map((rule) => ({ ...rule, enabled: true })),
      group.name,
    ),
    false,
  );
});

test('解散默认策略不把空组或不存在的组视为全部停用', () => {
  assert.equal(shouldClearRuleGroupByDefault([], '规则组 1'), false);
  const group = createRuleGroup([]);
  assert.equal(shouldClearRuleGroupByDefault(group.rules, '不存在的组'), false);
});

test('选择保留时仅移除成员关系，保留所有规则及其他组', () => {
  const first = createRuleGroup([]);
  const all = [...createRuleGroup(first.rules).rules, newRule('hold')];
  const before = structuredClone(all);
  const result = disbandRuleGroup(all, first.name);
  assert.equal(result.length, all.length);
  assert.deepEqual(
    result.slice(0, 2),
    before.slice(0, 2).map((rule) => ({ ...rule, group: null })),
  );
  assert.deepEqual(result.slice(2), before.slice(2));
  assert.deepEqual(all, before);
  assert.deepEqual(ruleGroupNames(result), ['规则组 2']);
});

test('勾选同时删除时仅删除目标组成员，其他组与未分组规则不受影响', () => {
  const first = createRuleGroup([]);
  const all = [...createRuleGroup(first.rules).rules, newRule('hold')];
  const before = structuredClone(all);
  const result = disbandRuleGroup(all, first.name, true);
  assert.deepEqual(result, before.slice(2));
  assert.deepEqual(all, before);
  assert.deepEqual(ruleGroupNames(result), ['规则组 2']);
});
