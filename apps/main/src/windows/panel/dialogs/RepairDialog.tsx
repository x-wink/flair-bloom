import { invoke } from '@tauri-apps/api/core';
import { useCallback, useEffect, useRef, useState } from 'react';
import Button from '../components/Button';
import { useConfirm } from '../components/ConfirmDialog';
import './dialog-base.css';
import './RepairDialog.css';

type Severity = 'info' | 'warn' | 'error';
type ItemStatus = 'ok' | 'orphan' | 'missing' | 'corrupted' | 'unknown';
type StepStatus = 'ok' | 'skipped' | 'failed' | 'pending_reboot';
type RepairCommand =
  | 'repair_dd_hid_residue'
  | 'repair_interception_residue'
  | 'repair_corrupted_profiles'
  | 'repair_clean_logs';

interface DiagnosticItem {
  id: string;
  category: string;
  label: string;
  severity: Severity;
  status: ItemStatus;
  detail: string;
  recommended_action: RepairCommand | null;
}

interface RepairReport {
  timestamp: string;
  items: DiagnosticItem[];
}

interface RepairStep {
  name: string;
  status: StepStatus;
  detail: string;
}

interface RepairOutcome {
  success: boolean;
  pending_reboot: boolean;
  summary: string;
  steps: RepairStep[];
  backup_dir: string | null;
}

interface Props {
  onClose: () => void;
  onToast: (kind: 'success' | 'warn' | 'error', message: string) => void;
}

const SEVERITY_LABEL: Record<Severity, string> = {
  info: '正常',
  warn: '注意',
  error: '异常',
};

const STATUS_LABEL: Record<ItemStatus, string> = {
  ok: '正常',
  orphan: '残留',
  missing: '缺失',
  corrupted: '损坏',
  unknown: '未知',
};

/// 仅当确有异常（severity 非 info 或 status 非 ok）时返回 chip 文案，
/// 正常项整体不挂 chip——卡片底色和边框已经传达「无异常」。
function itemChipLabel(severity: Severity, status: ItemStatus): string | null {
  if (status !== 'ok') return STATUS_LABEL[status];
  if (severity !== 'info') return SEVERITY_LABEL[severity];
  return null;
}

const STEP_LABEL: Record<StepStatus, string> = {
  ok: '完成',
  skipped: '跳过',
  failed: '失败',
  pending_reboot: '待重启',
};

const ACTION_LABEL: Record<RepairCommand, string> = {
  repair_dd_hid_residue: '清理究极HID 残留',
  repair_interception_residue: '清理 Interception 残留',
  repair_corrupted_profiles: '隔离损坏配置',
  repair_clean_logs: '清理旧日志',
};

const ACTION_CONFIRM: Record<RepairCommand, string> = {
  repair_dd_hid_residue:
    '将提权清理 PnP 注册、服务键、Driver Store 副本和驱动文件，并自动备份注册表。可能需要重启电脑。',
  repair_interception_residue:
    '将提权删除 Interception 的 keyboard / mouse 服务键并备份。完成后必须重启电脑。',
  repair_corrupted_profiles:
    '把无法解密的 .qzh 配置移到 corrupted/ 子目录，原文件保留可手动追回，不会删除。',
  repair_clean_logs: '将删除 7 天前的旧日志文件，崩溃日志（crash-*.log）会保留。',
};

export default function RepairDialog({ onClose, onToast }: Props) {
  const confirm = useConfirm();
  const [report, setReport] = useState<RepairReport | null>(null);
  const [scanning, setScanning] = useState(false);
  const [running, setRunning] = useState<RepairCommand | null>(null);
  const [outcomes, setOutcomes] = useState<Record<RepairCommand, RepairOutcome | null>>({
    repair_dd_hid_residue: null,
    repair_interception_residue: null,
    repair_corrupted_profiles: null,
    repair_clean_logs: null,
  });

  // onToast 通常是父组件内联函数，每次重渲染都会换引用。把它锁进 ref，
  // useEffect / useCallback 依赖列表里只放稳定 ref，避免父组件的 toast 状态
  // 变化（包括自动关闭）触发 diagnose_environment 重复执行。
  const onToastRef = useRef(onToast);
  useEffect(() => {
    onToastRef.current = onToast;
  }, [onToast]);

  const scan = useCallback(async () => {
    setScanning(true);
    try {
      const r = await invoke<RepairReport>('diagnose_environment');
      setReport(r);
    } catch (e) {
      onToastRef.current('error', `诊断失败：${e}`);
    } finally {
      setScanning(false);
    }
  }, []);

  useEffect(() => {
    void scan();
  }, [scan]);

  const runRepair = useCallback(
    async (cmd: RepairCommand) => {
      if (running) return;
      const proceed = await confirm({
        title: ACTION_LABEL[cmd],
        description: ACTION_CONFIRM[cmd],
        confirmText: '继续修复',
        cancelText: '取消',
        tone:
          cmd === 'repair_clean_logs' || cmd === 'repair_corrupted_profiles' ? 'default' : 'danger',
      });
      if (!proceed) return;
      setRunning(cmd);
      try {
        const result = await invoke<RepairOutcome>(cmd);
        setOutcomes((prev) => ({ ...prev, [cmd]: result }));
        if (result.success) {
          if (result.pending_reboot) {
            onToastRef.current('warn', `${result.summary}（请重启电脑）`);
          } else {
            onToastRef.current('success', result.summary);
          }
        } else {
          onToastRef.current('error', result.summary);
        }
        await scan();
      } catch (e) {
        onToastRef.current('error', `修复失败：${e}`);
      } finally {
        setRunning(null);
      }
    },
    [confirm, running, scan],
  );

  const grouped = groupByCategory(report?.items ?? []);
  const hasIssues = (report?.items ?? []).some((i) => i.severity !== 'info');

  return (
    <div className="repair-card">
      <header className="repair-header">
        <h2 className="repair-title">环境修复</h2>
        <p className="repair-tagline">
          检查驱动残留、配置文件完整性和日志体积。所有修复操作都先备份再执行。
        </p>
      </header>

      <div className="repair-body">
        {scanning && !report && <p className="repair-loading">正在诊断…</p>}

        {report && (
          <>
            <p className="repair-summary">
              {hasIssues ? (
                <span className="repair-summary--warn">检测到需要处理的项目</span>
              ) : (
                <span className="repair-summary--ok">未发现异常</span>
              )}
              <span className="repair-meta">扫描时间 {report.timestamp}</span>
            </p>
            {grouped.map(([category, items]) => (
              <section className="repair-section" key={category}>
                <p className="repair-section-label">{category}</p>
                <ul className="repair-list">
                  {items.map((it) => (
                    <li key={it.id} className={`repair-item repair-item--${it.severity}`}>
                      <div className="repair-item-head">
                        <span className="repair-item-label">{it.label}</span>
                        {(() => {
                          const chip = itemChipLabel(it.severity, it.status);
                          return chip ? (
                            <span className={`repair-flag repair-flag--${it.severity}`}>
                              {chip}
                            </span>
                          ) : null;
                        })()}
                      </div>
                      <p className="repair-item-detail">{it.detail}</p>
                      {it.recommended_action && (
                        <div className="repair-item-action">
                          <Button
                            size="sm"
                            variant="outline"
                            tone={it.severity === 'error' ? 'danger' : 'primary'}
                            loading={running === it.recommended_action}
                            disabled={running !== null && running !== it.recommended_action}
                            onClick={() => runRepair(it.recommended_action!)}
                          >
                            {ACTION_LABEL[it.recommended_action]}
                          </Button>
                        </div>
                      )}
                      {outcomes[it.recommended_action ?? 'repair_clean_logs'] &&
                        it.recommended_action && (
                          <RepairLog outcome={outcomes[it.recommended_action]!} />
                        )}
                    </li>
                  ))}
                </ul>
              </section>
            ))}
          </>
        )}
      </div>

      <div className="repair-actions">
        <Button variant="outline" onClick={() => void scan()} loading={scanning}>
          重新诊断
        </Button>
        <Button onClick={onClose}>关闭</Button>
      </div>
    </div>
  );
}

function RepairLog({ outcome }: { outcome: RepairOutcome }) {
  return (
    <div className="repair-log">
      <p className={`repair-log-summary repair-log-summary--${outcome.success ? 'ok' : 'fail'}`}>
        {outcome.summary}
      </p>
      <ul className="repair-log-steps">
        {outcome.steps.map((s, i) => (
          <li key={i} className={`repair-log-step repair-log-step--${s.status}`}>
            <span className="repair-log-step-name">{s.name}</span>
            <span className={`repair-flag repair-flag--step-${s.status}`}>
              {STEP_LABEL[s.status]}
            </span>
            <span className="repair-log-step-detail">{s.detail}</span>
          </li>
        ))}
      </ul>
      {outcome.backup_dir && <p className="repair-log-backup">备份目录：{outcome.backup_dir}</p>}
    </div>
  );
}

function groupByCategory(items: DiagnosticItem[]): Array<[string, DiagnosticItem[]]> {
  const map = new Map<string, DiagnosticItem[]>();
  for (const it of items) {
    const list = map.get(it.category) ?? [];
    list.push(it);
    map.set(it.category, list);
  }
  return Array.from(map.entries());
}
