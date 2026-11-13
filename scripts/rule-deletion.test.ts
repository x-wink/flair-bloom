import assert from 'node:assert/strict';
import test from 'node:test';
import { requestRuleDeletion } from '../apps/main/src/windows/panel/ruleDeletion.ts';

test('普通删除取消后保留规则，确认后只删除指定规则', async () => {
  const removed: string[] = [];
  const remove = (id: string) => { removed.push(id); };
  await requestRuleDeletion({ id: 'cancelled', confirm: async () => false, remove });
  assert.deepEqual(removed, []);
  const order: string[] = [];
  await requestRuleDeletion({ id: 'confirmed', confirm: async () => { order.push('confirm'); return true; }, remove: id => { order.push('remove'); remove(id); } });
  assert.deepEqual(order, ['confirm', 'remove']);
  assert.deepEqual(removed, ['confirmed']);
});

test('Ctrl 强删跳过确认且不记忆到下一次普通删除', async () => {
  const removed: string[] = [];
  const remove = (id: string) => { removed.push(id); };
  await requestRuleDeletion({ id: 'forced', force: true, confirm: async () => { assert.fail('强删不应弹出确认'); }, remove });
  let asked = 0;
  await requestRuleDeletion({ id: 'ordinary', confirm: async () => { asked++; return false; }, remove });
  assert.equal(asked, 1);
  assert.deepEqual(removed, ['forced']);
});
