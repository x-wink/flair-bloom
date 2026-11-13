import assert from 'node:assert/strict';
import test from 'node:test';
import { bindingDescription, targetIsForeground, filterBindableWindows, type WindowBindingStatus } from '../apps/main/src/windows/panel/windowBinding.ts';
import { matchesRuleFilter } from '../apps/main/src/windows/panel/ruleFilter.ts';
import { themePresetLabel } from '../apps/main/src/windows/panel/theme.ts';

test('绑定状态区分总开关、前台匹配、等待与关闭，不把已绑定误作已运行', () => {
  const status: WindowBindingStatus = { revision: 1, supported: true, binding: { exe: 'game.exe' }, target: { name: '游戏' }, open: false, present: true, reason: 'waiting', picking: false };
  assert.match(bindingDescription(status, false), /未运行/);
  assert.match(bindingDescription(status, true), /已暂停.*等待/);
  assert.match(bindingDescription({ ...status, present: false, reason: 'closed' }, true), /未打开/);
  assert.match(bindingDescription({ ...status, open: true, reason: 'matched' }, true), /已就绪/);
  assert.match(bindingDescription({ ...status, open: true, reason: 'self' }, true), /助手前台/);
  assert.equal(bindingDescription(undefined, true), '不限制窗口');
});

test('规则组筛选保留组内两种模式并排除未分组规则', () => {
  const rules = [{ mode: 'hold' as const, group: 'A' }, { mode: 'toggle' as const, group: 'A' }, { mode: 'toggle' as const, group: undefined }];
  assert.deepEqual(rules.filter(rule => matchesRuleFilter(rule, 'groups')), rules.slice(0, 2));
  assert.equal(rules.filter(rule => matchesRuleFilter(rule, 'all')).length, 3);
  assert.equal(rules.filter(rule => matchesRuleFilter(rule, 'hold')).length, 1);
  assert.equal(rules.filter(rule => matchesRuleFilter(rule, 'toggle')).length, 2);
});

test('主题标签使用颜色在前的统一格式', () => {
  assert.equal(themePresetLabel({ name: '霞紫', sect: '万花' }), '霞紫 · 万花');
});

test('窗口浮层按标题和程序名过滤，支持空白与多个词', () => {
  const windows = [{ id: 1, title: '剑网三 · 角色甲', exe: 'Game.EXE', class: 'Game' }, { id: 2, title: '记事本', exe: 'notepad.exe', class: 'Notepad' }];
  assert.equal(filterBindableWindows(windows, '  ').length, 2);
  assert.deepEqual(filterBindableWindows(windows, '剑网三 game.exe'), [windows[0]]);
  assert.deepEqual(filterBindableWindows(windows, 'NOTEpad'), [windows[1]]);
  assert.deepEqual(filterBindableWindows(windows, '其他窗口'), []);
});

test('目标图标仅跟随窗口前台状态，与全局启停及助手试运行闸门独立', () => {
  const status: WindowBindingStatus = { revision: 1, supported: true, binding: { exe: 'game.exe' }, open: true, foreground: true, present: true, reason: 'matched', picking: false };
  for (const enabled of [false, true]) {
    assert.equal(targetIsForeground(status), true);
    assert.match(bindingDescription(status, enabled), /目标窗口在前台/);
  }
  assert.equal(targetIsForeground({ ...status, foreground: false, reason: 'self' }), false);
  assert.equal(targetIsForeground({ ...status, foreground: false, open: false, reason: 'waiting' }), false);
});
