import type { CardRule } from './RuleCard';
import type { KeyId } from './components/KeyCapture';

export const DEFAULT_INTERVAL_MS = 10;

export function newRule(mode: CardRule['mode'] = 'hold', key?: KeyId): CardRule {
  const defaultKey: KeyId = key ?? { kind: 'keyboard', code: mode === 'hold' ? 0x51 : 0x46 };
  return {
    id: crypto.randomUUID(),
    enabled: mode === 'toggle',
    trigger_key: defaultKey,
    steps: [{ key: defaultKey, interval_ms: DEFAULT_INTERVAL_MS }],
    mode,
    stop_key: null,
    group: null,
  };
}
