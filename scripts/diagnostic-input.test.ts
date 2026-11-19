import assert from 'node:assert/strict';
import test from 'node:test';
import { summarizeSamples } from '../apps/main/src/windows/panel/diagnosticInput.ts';

test('面板送达计数按采样区间区分对照、驱动和失焦', () => {
  const rows = summarizeSamples(
    [
      { name: '轻量按键', start: 10, end: 15 },
      { name: '网吧滚轮', start: 20, end: 25 },
      { name: '网吧滚轮', start: 30, end: 35 },
    ],
    [
      { at: 9, kind: 'character' },
      { at: 10, kind: 'down' },
      { at: 12, kind: 'character' },
      { at: 15, kind: 'up' },
      { at: 18, kind: 'wheel' },
      { at: 21, kind: 'blur' },
      { at: 23, kind: 'wheel' },
      { at: 24, kind: 'wheel' },
      { at: 26, kind: 'blur' },
    ],
  );
  assert.deepEqual(rows, [
    {
      name: '轻量按键',
      attempts: 1,
      keyHits: 1,
      wheelHits: 0,
      down: 1,
      up: 1,
      character: 1,
      wheel: 0,
      blur: 0,
    },
    {
      name: '网吧滚轮',
      attempts: 2,
      keyHits: 0,
      wheelHits: 1,
      down: 0,
      up: 0,
      character: 0,
      wheel: 2,
      blur: 1,
    },
  ]);
});

test('一次重复和一次丢失不能抵消为三次送达', () => {
  const [row] = summarizeSamples(
    [0, 10, 20].map((start) => ({ name: '网吧按键', start, end: start + 5 })),
    [1, 2, 11].flatMap((at) =>
      (['down', 'up', 'character'] as const).map((kind) => ({ at, kind })),
    ),
  );
  assert.equal(row.character, 3);
  assert.equal(row.keyHits, 1);
});
