// 面板窗口等比缩放的纯计算：内容比例、工作区上限与存储净化。拖边锁比例在 Rust 侧 commands/window.rs。
import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  fitScale,
  LAYOUT_BASE,
  maxScale,
  sanitizeScales,
  scaledSize,
} from '../apps/main/src/windows/panel/windowScale.ts';

const V = LAYOUT_BASE.vertical;
const INF = Number.POSITIVE_INFINITY;

test('fitScale 取宽高比例里较小的，内容不溢出', () => {
  assert.equal(fitScale({ width: 810, height: 1000 }, V, INF), 1.39);
  assert.equal(fitScale({ width: 500, height: 1440 }, V, INF), 1.23);
});

test('fitScale 不低于 1、不超过上限', () => {
  assert.equal(fitScale({ width: 300, height: 500 }, V, INF), 1);
  assert.equal(fitScale({ width: 1215, height: 2160 }, V, 2), 2);
});

test('maxScale 按工作区装得下的最大等比尺寸，向下取两位，至少 1', () => {
  // 1920×1040 工作区装竖版：高度先到顶，1040 / 720 = 1.444…
  assert.equal(maxScale({ width: 1920, height: 1040 }, V), 1.44);
  assert.equal(maxScale({ width: 1280, height: 600 }, V), 1);
});

test('scaledSize 取整', () => {
  assert.deepEqual(scaledSize(V, 1.25), { width: 506, height: 900 });
});

test('sanitizeScales 把缺失、非数、小于 1 的值当 1', () => {
  assert.deepEqual(sanitizeScales(undefined), { vertical: 1, horizontal: 1 });
  assert.deepEqual(sanitizeScales({ vertical: 1.5, horizontal: 0.5 }), { vertical: 1.5, horizontal: 1 });
  assert.deepEqual(sanitizeScales({ vertical: 'x', horizontal: Number.NaN }), { vertical: 1, horizontal: 1 });
});
