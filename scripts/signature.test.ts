// 水印署名的显示判定：有权益且当前配置写了署名才覆盖默认署名。
import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  grantsSignature,
  type LicenseStatus,
  watermarkSignature,
} from '../apps/main/src/windows/panel/signature.ts';

const licensed: LicenseStatus = {
  id: 'lic_1',
  entitlements: ['flair.signature'],
  issued_at: 1,
  expires_at: null,
};
const otherEntitlement: LicenseStatus = { ...licensed, entitlements: ['flair.other'] };
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
