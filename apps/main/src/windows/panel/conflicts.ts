/**
 * 热键冲突检测。
 *
 * 规则：
 *  - ERROR  功能损坏：面板键遮蔽全局键；按压连发与切换连发共用同一触发/停止键
 *  - WARNING 行为意外：全局热键与规则触发键重叠（热键优先返回，该按键不触发连发）
 *
 * 不检测：
 *  - global_toggle == global_stop（有意的切换模式）
 *  - 全局热键 == target_key（target 走注入通道，hook 过滤自身注入）
 */

import { keyLabel, type KeyId } from './components/KeyCapture';

export type BurstMode = 'hold' | 'toggle';

export interface BurstRule {
  id: string;
  enabled: boolean;
  trigger_key: KeyId;
  target_key: KeyId;
  mode: BurstMode;
  stop_key: KeyId | null;
}

export interface Hotkeys {
  global_toggle: KeyId | null;
  global_stop: KeyId | null;
  panel_toggle: KeyId | null;
}

export type ConflictSeverity = 'error' | 'warning';

export type ConflictParticipant =
  | { kind: 'global'; field: 'global_toggle' | 'global_stop' | 'panel_toggle'; label: string }
  | {
      kind: 'rule';
      ruleId: string;
      ruleMode: BurstMode;
      field: 'trigger_key' | 'target_key';
      label: string;
    };

export interface Conflict {
  id: string;
  severity: ConflictSeverity;
  key: KeyId;
  message: string;
  participants: ConflictParticipant[];
}

// ── 工具 ─────────────────────────────────────────────────────────────────────

function keyEq(a: KeyId | null, b: KeyId | null): boolean {
  if (!a || !b) return false;
  if (a.kind !== b.kind) return false;
  return a.code === b.code;
}

function keyStr(k: KeyId): string {
  return `${k.kind}:${k.code}`;
}

const GLOBAL_LABELS: Record<string, string> = {
  global_toggle: '全局开启键',
  global_stop: '全局停止键',
  panel_toggle: '面板显隐键',
};

// ── 主算法 ────────────────────────────────────────────────────────────────────

export function detectConflicts(rules: BurstRule[], hotkeys: Hotkeys): Conflict[] {
  const conflicts: Conflict[] = [];

  // 仅检查已启用的规则
  const enabled = rules.filter((r) => r.enabled);

  // ── 1. 面板键遮蔽全局开启键 ────────────────────────────────────────────────
  if (keyEq(hotkeys.panel_toggle, hotkeys.global_toggle)) {
    conflicts.push({
      id: 'panel-shadows-toggle',
      severity: 'error',
      key: hotkeys.panel_toggle!,
      message: '面板显隐键与全局开启键相同，面板热键优先处理，全局开启键将失效',
      participants: [
        { kind: 'global', field: 'panel_toggle', label: GLOBAL_LABELS.panel_toggle },
        { kind: 'global', field: 'global_toggle', label: GLOBAL_LABELS.global_toggle },
      ],
    });
  }

  // ── 2. 面板键遮蔽全局停止键（且两者不同，避免与上条重复）──────────────────
  // global_stop 为 null 时有效值等同 global_toggle（切换模式），不需要单独检测
  if (
    hotkeys.global_stop &&
    !keyEq(hotkeys.global_stop, hotkeys.global_toggle) &&
    keyEq(hotkeys.panel_toggle, hotkeys.global_stop)
  ) {
    conflicts.push({
      id: 'panel-shadows-stop',
      severity: 'error',
      key: hotkeys.panel_toggle!,
      message: '面板显隐键与全局停止键相同，面板热键优先处理，全局停止键将失效',
      participants: [
        { kind: 'global', field: 'panel_toggle', label: GLOBAL_LABELS.panel_toggle },
        { kind: 'global', field: 'global_stop', label: GLOBAL_LABELS.global_stop },
      ],
    });
  }

  // ── 3. 全局热键与规则触发键重叠（warning）──────────────────────────────────
  // 构建需要检查的全局键列表（去重：toggle==stop 不重复报）
  const globalKeys: Array<{
    key: KeyId;
    field: 'global_toggle' | 'global_stop' | 'panel_toggle';
  }> = [];
  if (hotkeys.global_toggle) {
    globalKeys.push({ key: hotkeys.global_toggle, field: 'global_toggle' });
  }
  if (hotkeys.global_stop && !keyEq(hotkeys.global_stop, hotkeys.global_toggle)) {
    globalKeys.push({ key: hotkeys.global_stop, field: 'global_stop' });
  }
  if (hotkeys.panel_toggle) {
    globalKeys.push({ key: hotkeys.panel_toggle, field: 'panel_toggle' });
  }

  for (const { key, field } of globalKeys) {
    const clashing = enabled.filter((r) => keyEq(key, r.trigger_key));
    if (clashing.length === 0) continue;

    // 若该 key 已在第 1/2 条（ERROR）里出现，不再重复 warning
    const alreadyError = conflicts.some((c) => c.severity === 'error' && keyEq(c.key, key));
    if (alreadyError) continue;

    conflicts.push({
      id: `hotkey-masks-rule-${field}-${keyStr(key)}`,
      severity: 'warning',
      key,
      message: `${GLOBAL_LABELS[field]}与规则触发键相同，按下时热键优先，该按键不触发连发`,
      participants: [
        { kind: 'global', field, label: GLOBAL_LABELS[field] },
        ...clashing.map((r) => ({
          kind: 'rule' as const,
          ruleId: r.id,
          ruleMode: r.mode,
          field: 'trigger_key' as const,
          label: r.mode === 'hold' ? '按压连发' : '切换连发',
        })),
      ],
    });
  }

  // ── 4. 连发按键（target_key）== 另一条规则的触发键（warning）──────────────
  // 注入的 target_key 事件虽由 SIM_MARKER / PENDING_INJECTIONS 过滤，
  // 但 DD-HID 模式在极端情况下可能漏判，配置本身在概念上也令人困惑。
  for (const ruleA of enabled) {
    const cascading = enabled.filter(
      (ruleB) => ruleB.id !== ruleA.id && keyEq(ruleA.target_key, ruleB.trigger_key),
    );
    if (cascading.length === 0) continue;

    // 若该 key 已被更高优先级冲突覆盖，不重复报
    const alreadyCovered = conflicts.some(
      (c) =>
        keyEq(c.key, ruleA.target_key) &&
        c.participants.some((p) => p.kind === 'rule' && p.ruleId === ruleA.id),
    );
    if (alreadyCovered) continue;

    conflicts.push({
      id: `target-cascades-trigger-${ruleA.id}-${keyStr(ruleA.target_key)}`,
      severity: 'warning',
      key: ruleA.target_key,
      message: `连发按键 ${keyLabel(ruleA.target_key)} 同时是另一条规则的触发键，注入时可能意外激活该规则`,
      participants: [
        {
          kind: 'rule',
          ruleId: ruleA.id,
          ruleMode: ruleA.mode,
          field: 'target_key',
          label: '连发来源',
        },
        ...cascading.map((r) => ({
          kind: 'rule' as const,
          ruleId: r.id,
          ruleMode: r.mode,
          field: 'trigger_key' as const,
          label: '被触发',
        })),
      ],
    });
  }

  // ── 5. 按压连发与切换连发共用同一触发/停止键（error）──────────────────────
  // Hold 触发键物理按下时，同键的 Toggle 规则也会触发开关翻转，两种逻辑相互纠缠：
  //   · 第一次按键：hold 启动 + toggle 开启
  //   · 松开：hold 停止，toggle 持续
  //   · 第二次按键：hold 再次启动 + toggle 关闭（因为它在运行）
  // 用户无法独立控制两条规则，任何"按住连发"操作都会随机改变切换连发状态。
  // toggle 的 stop_key 如果等于 hold 触发键，效果相同（按住也会停止 toggle）。
  {
    const holdRules = enabled.filter((r) => r.mode === 'hold');
    const toggleRules = enabled.filter((r) => r.mode === 'toggle');
    // 每个冲突按键只报告一次
    const reportedKeys = new Set<string>();

    for (const hold of holdRules) {
      const holdKey = keyStr(hold.trigger_key);
      if (reportedKeys.has(holdKey)) continue;

      // toggle 触发键 == hold 触发键
      const byTrigger = toggleRules.filter((t) => keyEq(hold.trigger_key, t.trigger_key));
      // toggle 停止键（非 null 且与触发键不同）== hold 触发键
      const byStop = toggleRules.filter(
        (t) =>
          t.stop_key &&
          !keyEq(t.stop_key, t.trigger_key) &&
          keyEq(hold.trigger_key, t.stop_key) &&
          !byTrigger.some((bt) => bt.id === t.id),
      );
      const clashing = [...byTrigger, ...byStop];
      if (clashing.length === 0) continue;

      reportedKeys.add(holdKey);
      conflicts.push({
        id: `hold-toggle-key-${holdKey}`,
        severity: 'error',
        key: hold.trigger_key,
        message: `按键 ${keyLabel(hold.trigger_key)} 同时控制按压连发和切换连发，两种模式相互纠缠，行为不可预测`,
        participants: [
          {
            kind: 'rule',
            ruleId: hold.id,
            ruleMode: hold.mode,
            field: 'trigger_key',
            label: '按压连发触发键',
          },
          ...byTrigger.map((t) => ({
            kind: 'rule' as const,
            ruleId: t.id,
            ruleMode: t.mode,
            field: 'trigger_key' as const,
            label: '切换连发触发键',
          })),
          ...byStop.map((t) => ({
            kind: 'rule' as const,
            ruleId: t.id,
            ruleMode: t.mode,
            field: 'trigger_key' as const,
            label: '切换连发停止键',
          })),
        ],
      });
    }
  }

  return conflicts;
}

// ── 查询工具（供 UI 组件高亮具体控件）────────────────────────────────────────

/** 给定 KeyId，返回涉及它的最高冲突级别（用于 KeyCapture 控件着色）。 */
export function severityForKey(conflicts: Conflict[], key: KeyId | null): ConflictSeverity | null {
  if (!key) return null;
  const matching = conflicts.filter((c) => keyEq(c.key, key));
  if (matching.some((c) => c.severity === 'error')) return 'error';
  if (matching.some((c) => c.severity === 'warning')) return 'warning';
  return null;
}

/** 给定规则 ID，返回涉及该规则的最高冲突级别。 */
export function severityForRule(conflicts: Conflict[], ruleId: string): ConflictSeverity | null {
  const matching = conflicts.filter((c) =>
    c.participants.some((p) => p.kind === 'rule' && p.ruleId === ruleId),
  );
  if (matching.some((c) => c.severity === 'error')) return 'error';
  if (matching.some((c) => c.severity === 'warning')) return 'warning';
  return null;
}
