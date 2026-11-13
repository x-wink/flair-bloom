import assert from 'node:assert/strict';
import test from 'node:test';
import {
  FRAME_FLOW_COUNT,
  flowRibbon,
  framePoint,
  runningFrameGeometry,
  type RunningFrameVariant,
} from '../apps/main/src/windows/panel/runningFrame.ts';
import { visualEnhancementEnabled } from '../apps/main/src/windows/panel/visualEnhancement.ts';

test('视觉强化对缺失或无效设置默认开启，保留显式关闭', () => {
  for (const value of [undefined, null, 0, 1, '', 'false', {}, []])
    assert.equal(visualEnhancementEnabled(value), true);
  assert.equal(visualEnhancementEnabled(true), true);
  assert.equal(visualEnhancementEnabled(false), false);
});

test('圆角与直角轨迹按内边框尺寸计算，绕行闭合且距离连续', () => {
  const rectangle = runningFrameGeometry(104, 54, 0)!;
  assert.equal(rectangle.perimeter, 304);
  const circle = runningFrameGeometry(20, 20, 100)!;
  assert.equal(circle.radius, 8.5);
  assert.ok(Math.abs(circle.perimeter - 17 * Math.PI) < 1e-8);
  const capsule = runningFrameGeometry(104, 24, 100)!;
  assert.equal(capsule.radius, 10.5);
  assert.ok(Math.abs(capsule.perimeter - (160 + 21 * Math.PI)) < 1e-8);
  for (const frame of [rectangle, circle, capsule, runningFrameGeometry(1072, 603, 14)!]) {
    assert.deepEqual(framePoint(frame, 0), framePoint(frame, frame.perimeter));
    assert.deepEqual(framePoint(frame, -frame.perimeter), framePoint(frame, 0));
    for (let d = 0; d < frame.perimeter; d += 0.7) {
      const a = framePoint(frame, d),
        b = framePoint(frame, d + 0.7);
      assert.ok(Math.hypot(a.x - b.x, a.y - b.y) <= 0.700001);
      assert.ok(Math.abs(Math.hypot(a.nx, a.ny) - 1) < 1e-8);
    }
  }
});

const windows: [number, number, number, RunningFrameVariant][] = [
  [320, 46, 12, 'float'],
  [405, 720, 14, 'vertical'],
  [1072, 603, 14, 'horizontal'],
];
test('三类面板自动选取递增尺寸，两道流光保持一秒一圈与半圈间隔', () => {
  const frames = windows.map(([w, h, r, v]) => runningFrameGeometry(w, h, r, v)!);
  assert.deepEqual(
    frames.map((f) => f.beamLength),
    [120, 320, 480],
  );
  assert.deepEqual(
    frames.map((f) => f.headWidth),
    [8, 12, 14],
  );
  assert.equal(FRAME_FLOW_COUNT, 2);
  for (const frame of frames) {
    assert.equal(frame.duration, 1);
    const first = framePoint(frame, 0.123 * frame.perimeter);
    const opposite = framePoint(frame, 0.623 * frame.perimeter);
    assert.ok(Math.abs(first.x + opposite.x - frame.width - frame.inset * 2) < 1e-8);
    assert.ok(Math.abs(first.y + opposite.y - frame.height - frame.inset * 2) < 1e-8);
  }
});

test('倾斜渐细的尾端贴边，实心流光整圈都在窗口内', () => {
  for (const [w, h, r, v] of windows) {
    const frame = runningFrameGeometry(w, h, r, v)!;
    for (let phase = 0; phase < 1; phase += 0.007) {
      const distance = phase * frame.perimeter;
      const ribbon = flowRibbon(frame, distance);
      const tail = framePoint(frame, distance - frame.beamLength);
      assert.ok(Math.hypot(ribbon[0].x - tail.x, ribbon[0].y - tail.y) < 1e-8);
      const count = ribbon.length / 2;
      for (let i = 0; i < count; i++) {
        const offset =
          -frame.beamLength + ((frame.beamLength + frame.headWidth * 0.7) * i) / (count - 1);
        const edge = framePoint(frame, distance + offset);
        assert.ok(Math.hypot(ribbon[i].x - edge.x, ribbon[i].y - edge.y) < 1e-8);
      }
      for (const point of ribbon) {
        assert.ok(point.x >= 0 && point.x <= w && point.y >= 0 && point.y <= h);
      }
    }
  }
});

test('隐藏、过小或非法尺寸不生成流光轨迹', () => {
  for (const args of [
    [0, 720, 14],
    [405, 4, 14],
    [-1, 20, 12],
    [NaN, 720, 14],
    [405, Infinity, 14],
    [405, 720, NaN],
  ])
    assert.equal(runningFrameGeometry(args[0], args[1], args[2]), undefined);
});
