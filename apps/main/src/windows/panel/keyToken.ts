import type { KeyId } from './components/KeyCapture';

/** 按键的字符串标识，作 Map / Set 的键用；各处统一用这一份，格式变了只改这里。 */
export function keyToken(key: KeyId): string {
  return `${key.kind}:${key.code}`;
}
