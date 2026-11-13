import { useId, useRef, useState, type DragEventHandler } from 'react';
import ContextMenu, { type ContextMenuItem } from './components/ContextMenu';
import { ChevronIcon, TriangleIcon } from './components/icons';
import IntervalInput from './components/IntervalInput';
import KeyCapture, {
  type CaptureReject,
  keyEq,
  keyLabel,
  type KeyId,
  type KeyPolicies,
} from './components/KeyCapture';
import { severityForRuleField, type Conflict, type ConflictSeverity } from './conflicts';
import RuleWarning from './components/RuleWarning';
import SequenceEditor from './components/SequenceEditor';
import { isSimpleRule, ruleHotkeyDimming, type OutputStep } from './outputSequence';
import type { RuleProgress } from './useRuleStates';

type BurstMode = 'hold' | 'toggle';

/** 与 PanelApp 的 BurstRule 结构兼容，不反向依赖它。 */
export interface CardRule {
  id: string;
  enabled: boolean;
  trigger_key: KeyId;
  steps: OutputStep[];
  mode: BurstMode;
  stop_key: KeyId | null;
  group: string | null;
}

export interface CardDragHandlers {
  onDragStart: DragEventHandler<HTMLDivElement>;
  onDragOver: DragEventHandler<HTMLDivElement>;
  onDrop: DragEventHandler<HTMLDivElement>;
  onDragEnd: DragEventHandler<HTMLDivElement>;
}

interface Props {
  rule: CardRule;
  progress?: RuleProgress;
  isActive: boolean;
  isPaused: boolean;
  pauseReason?: string;
  /** 教程判定的「最后一条可见规则」（`lastRule`），承载教程锚点 `rule-latest`。 */
  isLatest: boolean;
  showAdvanced: boolean;
  isDragging: boolean;
  isDragTarget: boolean;
  conflict: ConflictSeverity | null;
  conflictDetails?: readonly Conflict[];
  policies: KeyPolicies;
  intervalMin: number;
  intervalMax: number;
  /** 全部已有分组名，供「移入组」子菜单。 */
  groups: string[];
  drag: CardDragHandlers;
  onPatch: (patch: Partial<CardRule>) => void;
  onSetMode: (mode: BurstMode) => void;
  onToggleAdvanced: () => void;
  onDelete: () => void;
  onMoveToGroup: (group: string) => void;
  onNewGroupWith: () => void;
  onRemoveFromGroup: () => void;
  onKeyReject: (info: CaptureReject) => void;
  /** 成功录入一次连发按键（教程实操判定用）。 */
  onTargetCaptured: () => void;
}

const MODE_NAME: Record<BurstMode, string> = { hold: '长按', toggle: '开关' };
const MODE_LABEL: Record<BurstMode, string> = { hold: '长按规则', toggle: '开关规则' };

function other(mode: BurstMode): BurstMode {
  return mode === 'hold' ? 'toggle' : 'hold';
}

export default function RuleCard({
  rule,
  progress,
  isActive,
  isPaused,
  pauseReason,
  isLatest,
  showAdvanced,
  isDragging,
  isDragTarget,
  conflict,
  conflictDetails,
  policies,
  intervalMin,
  intervalMax,
  groups,
  drag,
  onPatch,
  onSetMode,
  onToggleAdvanced,
  onDelete,
  onMoveToGroup,
  onNewGroupWith,
  onRemoveFromGroup,
  onKeyReject,
  onTargetCaptured,
}: Props) {
  const enableId = useId();
  const menuRef = useRef<HTMLButtonElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const multiple = rule.steps.length > 1;
  const isToggle = rule.mode === 'toggle';
  const dimming = ruleHotkeyDimming(rule);
  const canMute = !isActive && !isPaused;
  const triggerConflict = conflictDetails
    ? severityForRuleField(conflictDetails, rule.id, 'trigger_key')
    : conflict;
  const stopConflict = conflictDetails
    ? severityForRuleField(conflictDetails, rule.id, 'stop_key')
    : null;
  const targetConflict = conflictDetails
    ? severityForRuleField(conflictDetails, rule.id, 'target_key')
    : null;
  const conflictTitle =
    conflictDetails?.map((detail) => `${keyLabel(detail.key)}：${detail.message}`).join('\n') ||
    '请检查按键冲突';
  // 长按键与连发键分开时把「按住哪个键」提到主行：插队要按住的就是它，不能藏在高级设置里。
  const holdSplit = !isToggle && (multiple || !keyEq(rule.trigger_key, rule.steps[0].key));
  const nextMode = other(rule.mode);

  const moveTargets = groups.filter((g) => g !== rule.group);
  const menuItems: ContextMenuItem[] = [
    {
      label: '移入组',
      children: [
        ...moveTargets.map((g) => ({ label: g, onClick: () => onMoveToGroup(g) })),
        ...(moveTargets.length > 0 ? [{ type: 'divider' as const }] : []),
        { label: '新建规则组', onClick: onNewGroupWith },
      ],
    },
    ...(rule.group ? [{ label: '移出组', onClick: onRemoveFromGroup }] : []),
    { label: `改为${MODE_LABEL[nextMode]}`, onClick: () => onSetMode(nextMode) },
    { type: 'divider' },
    { label: '删除', danger: true, onClick: onDelete },
  ];

  const cls = [
    'rule-row',
    isToggle ? 'is-toggle' : 'is-hold',
    !rule.enabled && 'disabled',
    isActive && 'active',
    isPaused && 'is-paused',
    isDragging && 'dragging',
    isDragTarget && 'drag-target',
  ]
    .filter(Boolean)
    .join(' ');

  const targetPolicy =
    isToggle || holdSplit || showAdvanced ? policies.target : policies.trigger_target;

  return (
    <div
      className={cls}
      title={isPaused ? pauseReason || '已暂停：同组长按插队中，松手后恢复' : undefined}
      data-tour={isLatest ? 'rule-latest' : undefined}
      draggable
      {...drag}
    >
      <div className="rule-head">
        <div className="rule-title">
          <button
            type="button"
            className={`rule-mode-tag ${isToggle ? 'is-toggle' : 'is-hold'}`}
            data-tour="rule-mode"
            aria-label={`切换为${MODE_LABEL[nextMode]}`}
            title={`${MODE_LABEL[rule.mode]}（点击改为${MODE_LABEL[nextMode]}）`}
            onClick={() => onSetMode(nextMode)}
          >
            {MODE_NAME[rule.mode]}
          </button>
          {conflict && <RuleWarning className="rule-conflict-warning" title={conflictTitle} />}
        </div>
        <span className="drag-handle" aria-hidden>
          <svg width="18" height="10" viewBox="0 0 18 10" fill="currentColor">
            <circle cx="3" cy="2" r="1.2" />
            <circle cx="9" cy="2" r="1.2" />
            <circle cx="15" cy="2" r="1.2" />
            <circle cx="3" cy="8" r="1.2" />
            <circle cx="9" cy="8" r="1.2" />
            <circle cx="15" cy="8" r="1.2" />
          </svg>
        </span>
        <button
          ref={menuRef}
          type="button"
          className="rule-menu-btn"
          data-tour="rule-menu"
          aria-label="更多操作"
          title="更多操作"
          onClick={() => setMenuOpen((v) => !v)}
        >
          <ChevronIcon size={12} />
        </button>
      </div>
      <div className="rule-body">
        <div className="rule-main">
          {holdSplit && (
            <>
              <div className="rule-field">
                <label className={canMute && dimming.trigger ? 'is-muted' : undefined}>
                  长按键
                </label>
                <KeyCapture
                  onReject={onKeyReject}
                  policy={policies.trigger}
                  value={rule.trigger_key}
                  muted={canMute && dimming.trigger}
                  onChange={(vk) => vk && onPatch({ trigger_key: vk })}
                  conflict={triggerConflict}
                />
              </div>
              <span className="rule-arrow">→</span>
            </>
          )}
          <div className="rule-field" data-tour="rule-key">
            <label>{multiple ? '连发序列' : '连发按键'}</label>
            {multiple ? (
              <button
                type="button"
                className={`key-capture sequence-open${targetConflict === 'error' ? ' key-capture-error' : targetConflict === 'warning' ? ' key-capture-warn' : ''}`}
                onClick={onToggleAdvanced}
                aria-label="编辑连发序列"
                aria-expanded={showAdvanced}
                title="编辑连发序列"
              >
                {rule.steps
                  .slice(0, 3)
                  .map((step) => keyLabel(step.key))
                  .join(' → ')}
                {rule.steps.length > 3 ? ' …' : ''}
              </button>
            ) : (
              <KeyCapture
                onReject={onKeyReject}
                policy={targetPolicy}
                value={rule.steps[0].key}
                onChange={(vk) => {
                  if (!vk) return;
                  const patch: Partial<CardRule> = { steps: [{ ...rule.steps[0], key: vk }] };
                  if (!isToggle && !holdSplit && !showAdvanced) patch.trigger_key = vk;
                  onPatch(patch);
                  onTargetCaptured();
                }}
                conflict={
                  !isToggle && !holdSplit && !showAdvanced
                    ? (triggerConflict ?? targetConflict)
                    : targetConflict
                }
              />
            )}
          </div>
          {!multiple && (
            <div className="rule-field rule-interval" data-tour="rule-interval">
              <label>间隔</label>
              <IntervalInput
                value={rule.steps[0].interval_ms}
                min={intervalMin}
                max={intervalMax}
                onChange={(v) => onPatch({ steps: [{ ...rule.steps[0], interval_ms: v }] })}
              />
            </div>
          )}
        </div>
        <div className="rule-field rule-enable-field" data-tour="rule-enable">
          <label htmlFor={enableId}>启用</label>
          <input
            id={enableId}
            type="checkbox"
            className="enable-checkbox"
            checked={rule.enabled}
            onChange={(e) => onPatch({ enabled: e.target.checked })}
          />
        </div>
      </div>
      {progress && (multiple || progress.error || progress.max_lateness_ms > 0) && (
        <p className="sequence-progress" role="status">
          {progress.error ??
            `${isPaused ? '暂停' : '执行'}第 ${progress.step_index + 1}/${progress.step_count} 步 · 已完成 ${progress.completed_cycles} 轮${progress.max_lateness_ms > 0 ? ` · 最近延后 ${progress.lateness_ms}ms，最大 ${progress.max_lateness_ms}ms` : ''}`}
        </p>
      )}
      {showAdvanced && (
        <div className="rule-advanced">
          {isToggle || !holdSplit ? (
            <div className="rule-advanced-hotkeys">
              {isToggle && (
                <div className="rule-field">
                  <label className={canMute && dimming.trigger ? 'is-muted' : undefined}>
                    启动热键
                  </label>
                  <KeyCapture
                    onReject={onKeyReject}
                    policy={policies.trigger}
                    value={rule.trigger_key}
                    muted={canMute && dimming.trigger}
                    onChange={(vk) => vk && onPatch({ trigger_key: vk })}
                    conflict={triggerConflict}
                  />
                </div>
              )}
              <div className="rule-field">
                <label
                  className={
                    canMute && (isToggle ? dimming.stop : dimming.trigger) ? 'is-muted' : undefined
                  }
                >
                  {isToggle ? '停止热键' : '长按键'}
                </label>
                <KeyCapture
                  onReject={onKeyReject}
                  policy={policies.trigger}
                  value={isToggle ? (rule.stop_key ?? rule.trigger_key) : rule.trigger_key}
                  muted={canMute && (isToggle ? dimming.stop : dimming.trigger)}
                  onChange={(vk) =>
                    vk && onPatch(isToggle ? { stop_key: vk } : { trigger_key: vk })
                  }
                  conflict={isToggle ? stopConflict : triggerConflict}
                />
              </div>
            </div>
          ) : null}
          <SequenceEditor
            conflicts={conflictDetails?.filter((detail) =>
              detail.participants.some(
                (participant) =>
                  participant.kind === 'rule' &&
                  participant.ruleId === rule.id &&
                  participant.field === 'target_key',
              ),
            )}
            steps={rule.steps}
            policy={policies.target}
            min={intervalMin}
            max={intervalMax}
            onReject={onKeyReject}
            onChange={(steps) => onPatch({ steps })}
          />
        </div>
      )}
      <button
        className={`expand-btn${showAdvanced ? ' open' : ''}`}
        data-tour="rule-advanced"
        onClick={onToggleAdvanced}
        aria-label="高级设置"
        aria-expanded={showAdvanced}
      >
        {isSimpleRule(rule) ? (
          <ChevronIcon size={10} className="chevron" />
        ) : (
          <TriangleIcon size={10} className="chevron" />
        )}
        <span className="expand-label">{showAdvanced ? '收起高级设置' : '高级设置'}</span>
      </button>
      <ContextMenu
        open={menuOpen}
        onClose={() => setMenuOpen(false)}
        target={menuRef}
        items={menuItems}
        location="bottom-left"
      />
    </div>
  );
}
