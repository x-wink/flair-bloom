import assert from 'node:assert/strict';
import { test } from 'node:test';
import { TOURS } from '../apps/main/src/windows/panel/tour/tours/index.ts';
import { gettingStarted } from '../apps/main/src/windows/panel/tour/tours/getting-started.ts';
import { lastRule } from '../apps/main/src/windows/panel/tour/tours/helpers.ts';
import type { TourHost, TourRule, TourSnapshot } from '../apps/main/src/windows/panel/tour/types.ts';

const key = { kind: 'keyboard' as const, code: 70 };
const rule = (mode: TourRule['mode'], id: string = mode): TourRule => ({
  id, mode, enabled: mode === 'toggle', trigger_key: key,
  steps: [{ key, interval_ms: 10 }], group: null,
});
const snapshot = (rules: TourRule[] = []): TourSnapshot => ({
  rules, globalEnabled: false, layout: 'vertical', filter: 'all',
  collapsedGroups: [], keyCaptureSeq: 0, coincidentToggle: true, ruleTriggerLabel: 'F',
});
function host(now: TourSnapshot, calls: string[] = []): TourHost {
  return {
    snapshot: now,
    setFilter: value => { calls.push(`filter:${value}`); },
    setLayout: async value => { calls.push(`layout:${value}`); },
    closeMenus: () => { calls.push('close'); },
    openCreateMenu: () => { calls.push('create'); },
    openSettings: () => {}, closeSettings: () => {},
    openJx3: () => {}, closeJx3: () => {},
  };
}

test('全部教程最多三句，每句不超过十字，目录与提示也保持简短', () => {
  assert.equal(TOURS.length, 8);
  const short = (text: string) => assert.ok(Array.from(text).length <= 10, `文案超长：${text}`);
  for (const tour of TOURS) {
    short(tour.title);
    short(tour.summary);
    assert.ok(tour.steps.length <= 3, `${tour.id} 步骤过多`);
    for (const mode of ['hold', 'toggle'] as const) {
      const current = snapshot([rule(mode)]);
      for (const step of tour.steps) {
        short(step.title);
        if (step.actionHint) short(step.actionHint);
        const lines = typeof step.body === 'function' ? step.body(host(current)) : step.body;
        assert.ok(lines.length > 0 && lines.length <= 3, `${tour.id}/${step.id} 句数不符`);
        lines.forEach(short);
      }
      const reason = tour.available?.({ ...current, coincidentToggle: false });
      if (reason) short(reason);
    }
  }
});

test('上手恰好三步，首步展示可任选的规则菜单', async () => {
  assert.equal(gettingStarted.steps.length, 3);
  const first = gettingStarted.steps[0];
  assert.deepEqual(first.target, ['add-toggle', 'add-hold']);
  const calls: string[] = [];
  await first.prepare?.(host(snapshot(), calls));
  assert.deepEqual(calls, ['close', 'layout:vertical', 'filter:all', 'create']);
  for (const mode of ['toggle', 'hold'] as const) {
    assert.equal(first.done?.(snapshot([rule(mode)]), snapshot()), true);
  }
  assert.equal(first.done?.(snapshot(), snapshot()), false);
});

test('录键与启用必须作用于高亮的同一条规则，录入原按键也能完成', () => {
  const configure = gettingStarted.steps[1];
  for (const mode of ['toggle', 'hold'] as const) {
    const entered = snapshot([rule(mode)]);
    assert.equal(configure.done?.(entered, entered), false);
    const recorded = { ...entered, keyCaptureSeq: 1 };
    assert.equal(configure.done?.(recorded, entered), mode === 'toggle');
    const enabled = { ...recorded, rules: [{ ...rule(mode), enabled: true }] };
    assert.equal(configure.done?.(enabled, entered), true);
    assert.equal(configure.done?.({ ...enabled, rules: [{ ...rule(mode, 'other'), enabled: true }] }, entered), false);
  }
});

test('最后一步根据所选规则提示按一次或按住', () => {
  const last = gettingStarted.steps[2];
  assert.equal(typeof last.body, 'function');
  if (typeof last.body !== 'function') return;
  assert.deepEqual(last.body(host(snapshot([rule('toggle')]))), ['点击开始运行', '按F开始连发', '再按一次就停止']);
  assert.deepEqual(last.body(host(snapshot([rule('hold')]))), ['点击开始运行', '按住F连发', '松手即可停止']);
  assert.deepEqual(last.body(host({ ...snapshot([rule('toggle')]), ruleTriggerLabel: '较长的特殊触发按键' })), ['点击开始运行', '按触发键开始连发', '再按一次就停止']);
  assert.equal(last.done?.(snapshot(), snapshot()), false);
  assert.equal(last.done?.({ ...snapshot(), globalEnabled: true }, snapshot()), true);
});

test('缺少规则时跳过录键，已运行时跳过开始步骤，折叠规则不作为高亮目标', async () => {
  assert.equal(await gettingStarted.steps[1].prepare?.(host(snapshot())), 'skip');
  assert.equal(await gettingStarted.steps[2].prepare?.(host({ ...snapshot(), globalEnabled: true })), 'skip');
  const visible = rule('toggle', 'visible');
  const hidden = { ...rule('hold', 'hidden'), group: '折叠组' };
  assert.equal(lastRule({ ...snapshot([visible, hidden]), collapsedGroups: ['折叠组'] })?.id, visible.id);
});
