import assert from 'node:assert/strict';
import { test } from 'node:test';
import { targetBounds } from '../apps/main/src/windows/panel/tour/targetGeometry.ts';

test('多个目标合成包围盒，包含间隔但不扩展到后续菜单项', () => {
  const toggle = { left: 20, top: 100, width: 180, height: 32 };
  const hold = { left: 20, top: 138, width: 180, height: 32 };
  const result = targetBounds([toggle, hold]);
  assert.deepEqual(result, { left: 20, top: 100, width: 180, height: 70 });
  assert.deepEqual(targetBounds([hold, toggle]), result);
  assert.ok(result && result.top + result.height < 180);
});

test('单目标保留原边界，滚动后按真实坐标重算', () => {
  const element = { left: 10, top: -20, width: 100, height: 30 };
  assert.deepEqual(targetBounds([element]), element);
  assert.deepEqual(targetBounds([element, { left: 150, top: 40, width: 50, height: 20 }]), { left: 10, top: -20, width: 190, height: 80 });
});

test('空集合或不可见目标不创建遮罩开口', () => {
  assert.equal(targetBounds([]), undefined);
  assert.equal(targetBounds([{ left: 10, top: 10, width: 0, height: 20 }]), undefined);
  assert.equal(targetBounds([{ left: 10, top: 10, width: 20, height: 0 }]), undefined);
});
