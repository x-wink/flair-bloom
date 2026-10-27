// 兑换码许可证的前端判定：水印署名取值与激活错误文案。
import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  activationErrorText,
  clampSignature,
  entitlementInfo,
  grantsSignature,
  type LicenseSummary,
  watermarkSignature,
} from '../apps/main/src/windows/panel/license.ts';

const licensed: LicenseSummary = {
  entitlements: [{ key: 'flair.signature', expires_at: null }],
  licenses: [{ id: 'lic_1', entitlements: ['flair.signature'], issued_at: 1, expires_at: null }],
};
const otherEntitlement: LicenseSummary = {
  entitlements: [{ key: 'flair.other', expires_at: null }],
  licenses: [],
};
const profiles = [
  { meta: { name: '主力', signature: '亲友 · 小花' } },
  { meta: { name: '空署名', signature: '' } },
  { meta: { name: '旧配置' } },
];

test('有权益且当前配置有署名时覆盖', () => {
  assert.equal(watermarkSignature(licensed, profiles, '主力'), '亲友 · 小花');
});

test('没有权益时不覆盖，即使配置里有署名', () => {
  assert.equal(watermarkSignature(undefined, profiles, '主力'), undefined);
  assert.equal(watermarkSignature(otherEntitlement, profiles, '主力'), undefined);
});

test('署名为空、缺字段或配置不在列表里时回落默认', () => {
  assert.equal(watermarkSignature(licensed, profiles, '空署名'), undefined);
  assert.equal(watermarkSignature(licensed, profiles, '旧配置'), undefined);
  assert.equal(watermarkSignature(licensed, profiles, '不存在'), undefined);
});

test('权益判定只认 flair.signature', () => {
  assert.equal(grantsSignature(licensed), true);
  assert.equal(grantsSignature(otherEntitlement), false);
  assert.equal(grantsSignature(undefined), false);
});

test('激活错误按 kind 出文案', () => {
  assert.equal(activationErrorText({ kind: 'invalid_code' }), '兑换码不对，检查一下有没有输错');
  assert.equal(
    activationErrorText({ kind: 'rate_limited', retry_after_secs: 60 }),
    '尝试太频繁，请 60 秒后再试',
  );
  assert.equal(
    activationErrorText({ kind: 'rate_limited', retry_after_secs: null }),
    '尝试太频繁，请稍后再试',
  );
  assert.equal(
    activationErrorText({ kind: 'failed', message: '兑换码不存在', trace_id: 't' }),
    '激活失败：兑换码不存在',
  );
});

test('非 ActivationError 形状的异常也有兜底文案', () => {
  // invoke 被拒时也可能是普通字符串（命令未注册等），不能显示 undefined
  assert.equal(activationErrorText(undefined), '激活失败，请稍后再试');
  assert.equal(activationErrorText('boom' as never), '激活失败，请稍后再试');
});

test('认识的权益有说明与入口，不认识的提示更新', () => {
  const known = entitlementInfo('flair.signature');
  assert.equal(known.name, '自定义水印署名');
  assert.equal(known.action?.target, 'settings-profiles');
  const unknown = entitlementInfo('flair.future');
  assert.equal(unknown.name, 'flair.future');
  assert.match(unknown.description, /更新应用/);
  assert.equal(unknown.action, undefined);
});

test('有期限的署名权益到期后不再生效', () => {
  const expiring: LicenseSummary = {
    entitlements: [{ key: 'flair.signature', expires_at: 1000 }],
    licenses: [],
  };
  assert.equal(grantsSignature(expiring, 999_000), true);
  assert.equal(grantsSignature(expiring, 1_000_000), false);
  assert.equal(watermarkSignature(expiring, profiles, '主力'), undefined);
});

test('署名按字符截断，emoji 算一个', () => {
  assert.equal(clampSignature('🌸'.repeat(20)), '🌸'.repeat(16));
  assert.equal(clampSignature('短署名'), '短署名');
});
