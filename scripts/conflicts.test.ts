import assert from 'node:assert/strict';
import { test } from 'node:test';
import { detectConflicts, severityForKey, severityForRule, severityForRuleField, type BurstRule, type Hotkeys } from '../apps/main/src/windows/panel/conflicts.ts';

const key = (code: number) => ({ kind: 'keyboard' as const, code });
const f1 = key(112);
const f2 = key(113);
const q = key(81);
const e = key(69);
const empty: Hotkeys = { global_toggle: null, global_stop: null, panel_toggle: null };
const rule = (patch: Partial<BurstRule> = {}): BurstRule => ({ id: 'rule', mode: 'toggle', enabled: true, trigger_key: q, stop_key: e, steps: [{ key: q, interval_ms: 10 }], ...patch });

test('独立停止键与任一种全局热键重叠时，检查与高亮只落在停止字段', () => {
  for (const field of ['global_toggle', 'global_stop', 'panel_toggle'] as const) {
    const conflicts = detectConflicts([rule({ stop_key: f1 })], { ...empty, [field]: f1 });
    assert.equal(conflicts.length, 1);
    assert.equal(severityForRule(conflicts, 'rule'), 'warning');
    assert.equal(severityForRuleField(conflicts, 'rule', 'stop_key'), 'warning');
    assert.equal(severityForRuleField(conflicts, 'rule', 'trigger_key'), null);
    assert.equal(severityForRuleField(conflicts, 'rule', 'target_key'), null);
    assert.equal(severityForKey(conflicts, f1), 'warning');
    assert.match(conflicts[0].message, /停止键/);
  }
});

test('默认同键启停同时标记两种角色，全局启停同键不重复报告', () => {
  const conflicts = detectConflicts([rule({ trigger_key: f1, stop_key: null })], { ...empty, global_toggle: f1, global_stop: f1 });
  assert.equal(conflicts.length, 1);
  assert.equal(severityForRuleField(conflicts, 'rule', 'trigger_key'), 'warning');
  assert.equal(severityForRuleField(conflicts, 'rule', 'stop_key'), 'warning');
  assert.match(conflicts[0].message, /触发键、停止键/);
});

test('独立启动键提示区分未开启与已开启，全局停止键说明优先停止全部规则', () => {
  const start = detectConflicts([rule({ trigger_key: f1 })], { ...empty, global_toggle: f1, global_stop: f2 });
  assert.match(start[0].message, /未开启时热键优先/);
  assert.match(start[0].message, /已开启时可能触发或停止同键规则/);
  const stop = detectConflicts([rule({ stop_key: f2 })], { ...empty, global_toggle: f1, global_stop: f2 });
  assert.match(stop[0].message, /优先停止全部规则/);
});

test('全局热键之间的错误不再掩盖规则本身的重叠警告', () => {
  const conflicts = detectConflicts([rule({ trigger_key: f1 })], { ...empty, global_toggle: f1, panel_toggle: f1 });
  assert.equal(severityForKey(conflicts, f1), 'error');
  assert.equal(severityForRule(conflicts, 'rule'), 'warning');
  assert.equal(severityForRuleField(conflicts, 'rule', 'trigger_key'), 'warning');
});

test('连发来源与被触发规则均保留诊断，不被同键的其他警告遮蔽', () => {
  const origin = rule({ id: 'origin', trigger_key: f1, stop_key: null, steps: [{ key: f1, interval_ms: 10 }] });
  const destination = rule({ id: 'destination', trigger_key: f1, stop_key: null, steps: [{ key: e, interval_ms: 10 }] });
  const conflicts = detectConflicts([origin, destination], { ...empty, global_toggle: f1 });
  assert.equal(severityForRuleField(conflicts, 'origin', 'target_key'), 'warning');
  assert.equal(severityForRuleField(conflicts, 'destination', 'trigger_key'), 'warning');
  assert.equal(severityForRuleField(conflicts, 'destination', 'target_key'), null);
});

test('连发目标与任一种全局热键同键时只标记输出字段', () => {
  for (const field of ['global_toggle', 'global_stop', 'panel_toggle'] as const) {
    const conflicts = detectConflicts([rule({ steps: [{ key: f1, interval_ms: 10 }] })], { ...empty, [field]: f1 });
    assert.equal(conflicts.length, 1);
    assert.equal(severityForRuleField(conflicts, 'rule', 'target_key'), 'warning');
    assert.equal(severityForRuleField(conflicts, 'rule', 'trigger_key'), null);
    assert.equal(severityForRuleField(conflicts, 'rule', 'stop_key'), null);
    assert.match(conflicts[0].message, /模拟输出通常会被过滤/);
  }
});

test('检查序列中的全部目标，重复输出和同键全局启停不重复报告', () => {
  const source = rule({ steps: [{ key: q, interval_ms: 10 }, { key: f1, interval_ms: 20 }, { key: f1, interval_ms: 30 }] });
  const before = structuredClone(source);
  const conflicts = detectConflicts([source], { ...empty, global_toggle: f1, global_stop: f1 });
  assert.equal(conflicts.length, 1);
  assert.equal(severityForRuleField(conflicts, 'rule', 'target_key'), 'warning');
  assert.deepEqual(source, before);
});

test('停用规则和长按的无效停止字段均不误报', () => {
  assert.deepEqual(detectConflicts([rule({ enabled: false, trigger_key: f1, stop_key: f1 })], { ...empty, global_toggle: f1 }), []);
  assert.deepEqual(detectConflicts([rule({ mode: 'hold', stop_key: f1 })], { ...empty, global_toggle: f1 }), []);
  assert.deepEqual(detectConflicts([], empty), []);
});

test('字段查询不把别的规则或无关字段的冲突转嫁过来', () => {
  const conflicts = detectConflicts([rule({ trigger_key: f1, stop_key: q })], { ...empty, panel_toggle: f1 });
  assert.equal(severityForRuleField(conflicts, 'other', 'trigger_key'), null);
  assert.equal(severityForRuleField(conflicts, 'rule', 'stop_key'), null);
  assert.equal(severityForKey(conflicts, null), null);
  const error = { ...conflicts[0], severity: 'error' as const };
  assert.equal(severityForRuleField([...conflicts, error], 'rule', 'trigger_key'), 'error');
  assert.equal(severityForRule([...conflicts, error], 'rule'), 'error');
});
