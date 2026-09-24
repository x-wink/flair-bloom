// 横版走线几何的纯函数断言。前端没有测试框架，用 Node 内置 test runner + 类型剥离直接跑：
// `pnpm test:ui`。
import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  buildWires,
  chamferPath,
  FAN_GAP,
  fanOffset,
  placeWires,
  routeWire,
  wiredKeys,
  type WireRule,
} from '../apps/main/src/windows/panel/hkbWires.ts';
import { keyToken } from '../apps/main/src/windows/panel/keyToken.ts';

const kb = (code: number) => ({ kind: 'keyboard', code }) as unknown as WireRule['trigger_key'];

function rule(
  id: string,
  trigger: number,
  target: number,
  mode: 'hold' | 'toggle' = 'hold',
  stop?: number,
): WireRule {
  return {
    id,
    enabled: true,
    trigger_key: kb(trigger),
    target_key: kb(target),
    mode,
    stop_key: stop === undefined ? null : kb(stop),
  };
}

test('trigger == target 的单键规则不出边', () => {
  assert.deepEqual(buildWires([rule('a', 0x51, 0x51), rule('b', 0x45, 0x45, 'toggle')]), []);
});

test('trigger != target 出一条 target 边，保留模式与启用态', () => {
  const r = { ...rule('a', 0x51, 0x45, 'toggle'), enabled: false };
  assert.deepEqual(buildWires([r]), [
    {
      ruleId: 'a',
      from: keyToken(kb(0x51)),
      to: keyToken(kb(0x45)),
      kind: 'target',
      mode: 'toggle',
      enabled: false,
    },
  ]);
});

test('切换连发独立停止键出 stop 边，停止键 == 启动键不出', () => {
  const wires = buildWires([rule('a', 0x51, 0x51, 'toggle', 0x58), rule('b', 0x46, 0x46, 'toggle', 0x46)]);
  assert.equal(wires.length, 1);
  assert.equal(wires[0].kind, 'stop');
  assert.equal(wires[0].to, keyToken(kb(0x58)));
});

test('长按规则的 stop_key 不出边', () => {
  assert.deepEqual(buildWires([rule('a', 0x51, 0x51, 'hold', 0x58)]), []);
});

test('参与键集合是所有边的两端', () => {
  const keys = wiredKeys(buildWires([rule('a', 0x51, 0x45), rule('b', 0x46, 0x46, 'toggle', 0x58)]));
  assert.deepEqual(
    [...keys].sort(),
    [kb(0x51), kb(0x45), kb(0x46), kb(0x58)].map(keyToken).sort(),
  );
});

test('并行扇出偏移居中对称、间距为 FAN_GAP', () => {
  assert.equal(fanOffset(0, 1), 0);
  const three = [0, 1, 2].map((i) => fanOffset(i, 3));
  assert.deepEqual(three, [-FAN_GAP, 0, FAN_GAP]);
  const four = [0, 1, 2, 3].map((i) => fanOffset(i, 4));
  assert.equal(four[0], -four[3]);
  assert.equal(four[1], -four[2]);
  assert.equal(four[1] - four[0], FAN_GAP);
});

test('跨行走线：竖 → 横 → 竖，起点下沿、终点上沿', () => {
  const from = { left: 0, top: 0, width: 40, height: 40 };
  const to = { left: 100, top: 100, width: 40, height: 40 };
  assert.deepEqual(routeWire(from, to, 0), [
    [20, 40],
    [20, 70],
    [120, 70],
    [120, 100],
  ]);
});

test('同一行走行上方的通道，扇出同时平移起止点与通道', () => {
  const from = { left: 0, top: 50, width: 40, height: 40 };
  const to = { left: 100, top: 50, width: 40, height: 40 };
  assert.deepEqual(routeWire(from, to, 2), [
    [22, 50],
    [22, 44],
    [122, 44],
    [122, 50],
  ]);
});

test('切角路径：两端不动，每个拐角多出一段 45° 斜边', () => {
  const d = chamferPath([
    [0, 0],
    [0, 30],
    [50, 30],
    [50, 60],
  ]);
  assert.equal(d, 'M0 0 L0 24 L6 30 L44 30 L50 36 L50 60');
});

test('空折线不出路径', () => {
  assert.equal(chamferPath([]), '');
});

test('悬停键：出去的与指向它的分开；端点不在图上的不出路径', () => {
  const wires = buildWires([
    rule('out1', 0x51, 0x45),
    rule('out2', 0x51, 0x52),
    rule('in1', 0x46, 0x51),
    rule('offmap', 0x51, 0x5a),
  ]);
  const rects = new Map([
    [keyToken(kb(0x51)), { left: 0, top: 0, width: 40, height: 40 }],
    [keyToken(kb(0x45)), { left: 200, top: 100, width: 40, height: 40 }],
    [keyToken(kb(0x52)), { left: 100, top: 100, width: 40, height: 40 }],
    [keyToken(kb(0x46)), { left: 300, top: 100, width: 40, height: 40 }],
  ]);
  const placed = placeWires(wires, keyToken(kb(0x51)), (t) => rects.get(t));
  assert.deepEqual(
    placed.map((p) => `${p.dir}:${p.wire.ruleId}`),
    ['out:out2', 'out:out1', 'in:in1'],
  );
  // 两条出线扇出：按目标横坐标排序后对称偏移
  assert.equal(placed[0].start[0], 20 - FAN_GAP / 2);
  assert.equal(placed[1].start[0], 20 + FAN_GAP / 2);
});
