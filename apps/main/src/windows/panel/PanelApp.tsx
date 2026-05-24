import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { LazyStore } from '@tauri-apps/plugin-store';
import { useEffect, useState } from 'react';
import iconUrl from '../../assets/icon-32.png';
import bgUrl from '../../assets/icon-bg.png';
import { useConfirm } from './ConfirmDialog';
import './PanelApp.css';
import { useToast } from './Toast';

type CloseBehavior = 'minimize' | 'exit';
const settingsStore = new LazyStore('settings.json');
const CLOSE_BEHAVIOR_KEY = 'closeBehavior';
const ACTIVE_TAB_KEY = 'activeTab';

type BurstMode = 'hold' | 'toggle';

interface BurstRule {
  id: string;
  enabled: boolean;
  trigger_key: number;
  target_key: number;
  mode: BurstMode;
  stop_key: number | null;
  interval_ms: number;
}

const KEY_NAMES: Record<number, string> = {
  0x41: 'A',
  0x42: 'B',
  0x43: 'C',
  0x44: 'D',
  0x45: 'E',
  0x46: 'F',
  0x47: 'G',
  0x48: 'H',
  0x49: 'I',
  0x4a: 'J',
  0x4b: 'K',
  0x4c: 'L',
  0x4d: 'M',
  0x4e: 'N',
  0x4f: 'O',
  0x50: 'P',
  0x51: 'Q',
  0x52: 'R',
  0x53: 'S',
  0x54: 'T',
  0x55: 'U',
  0x56: 'V',
  0x57: 'W',
  0x58: 'X',
  0x59: 'Y',
  0x5a: 'Z',
  0x30: '0',
  0x31: '1',
  0x32: '2',
  0x33: '3',
  0x34: '4',
  0x35: '5',
  0x36: '6',
  0x37: '7',
  0x38: '8',
  0x39: '9',
  0x70: 'F1',
  0x71: 'F2',
  0x72: 'F3',
  0x73: 'F4',
  0x74: 'F5',
  0x75: 'F6',
  0x76: 'F7',
  0x77: 'F8',
  0x78: 'F9',
  0x79: 'F10',
  0x7a: 'F11',
  0x7b: 'F12',
  0x20: 'Space',
  0x0d: 'Enter',
  0x1b: 'Esc',
  0x08: 'Backspace',
  0x09: 'Tab',
  0x26: '↑',
  0x28: '↓',
  0x25: '←',
  0x27: '→',
};

const BROWSER_VK: Record<string, number> = {
  KeyA: 0x41,
  KeyB: 0x42,
  KeyC: 0x43,
  KeyD: 0x44,
  KeyE: 0x45,
  KeyF: 0x46,
  KeyG: 0x47,
  KeyH: 0x48,
  KeyI: 0x49,
  KeyJ: 0x4a,
  KeyK: 0x4b,
  KeyL: 0x4c,
  KeyM: 0x4d,
  KeyN: 0x4e,
  KeyO: 0x4f,
  KeyP: 0x50,
  KeyQ: 0x51,
  KeyR: 0x52,
  KeyS: 0x53,
  KeyT: 0x54,
  KeyU: 0x55,
  KeyV: 0x56,
  KeyW: 0x57,
  KeyX: 0x58,
  KeyY: 0x59,
  KeyZ: 0x5a,
  Digit0: 0x30,
  Digit1: 0x31,
  Digit2: 0x32,
  Digit3: 0x33,
  Digit4: 0x34,
  Digit5: 0x35,
  Digit6: 0x36,
  Digit7: 0x37,
  Digit8: 0x38,
  Digit9: 0x39,
  F1: 0x70,
  F2: 0x71,
  F3: 0x72,
  F4: 0x73,
  F5: 0x74,
  F6: 0x75,
  F7: 0x76,
  F8: 0x77,
  F9: 0x78,
  F10: 0x79,
  F11: 0x7a,
  F12: 0x7b,
  Space: 0x20,
  Enter: 0x0d,
  Escape: 0x1b,
  Backspace: 0x08,
  Tab: 0x09,
  ArrowUp: 0x26,
  ArrowDown: 0x28,
  ArrowLeft: 0x25,
  ArrowRight: 0x27,
};

function keyLabel(vk: number): string {
  return KEY_NAMES[vk] ?? (vk ? `0x${vk.toString(16).toUpperCase()}` : '—');
}

function newRule(mode: BurstMode = 'hold'): BurstRule {
  const isHold = mode === 'hold';
  const vk = isHold ? 0x51 : 0x46;
  return {
    id: crypto.randomUUID(),
    enabled: !isHold,
    trigger_key: vk,
    target_key: vk,
    mode,
    stop_key: null,
    interval_ms: 10,
  };
}

function defaultRules(): BurstRule[] {
  return [newRule('hold'), newRule('toggle')];
}

function KeyCapture({ value, onChange }: { value: number; onChange: (vk: number) => void }) {
  const [capturing, setCapturing] = useState(false);

  function handleKeyDown(e: React.KeyboardEvent) {
    e.preventDefault();
    const vk = BROWSER_VK[e.code];
    if (vk) {
      onChange(vk);
      setCapturing(false);
    }
  }

  return (
    <button
      className={`key-capture${capturing ? ' capturing' : ''}`}
      onKeyDown={capturing ? handleKeyDown : undefined}
      onClick={() => setCapturing(true)}
      onBlur={() => setCapturing(false)}
    >
      {capturing ? '按下按键…' : keyLabel(value)}
    </button>
  );
}

export default function PanelApp() {
  const [globalEnabled, setGlobalEnabled] = useState(false);
  const [rules, setRules] = useState<BurstRule[]>([]);
  const [advancedOpen, setAdvancedOpen] = useState<Record<string, boolean>>({});
  const [activeTab, setActiveTab] = useState<BurstMode>('toggle');
  const confirm = useConfirm();
  const toast = useToast();

  useEffect(() => {
    settingsStore
      .get<BurstMode>(ACTIVE_TAB_KEY)
      .then((v) => {
        if (v === 'hold' || v === 'toggle') setActiveTab(v);
      })
      .catch(() => {});

    invoke<boolean>('get_global_enabled')
      .then(setGlobalEnabled)
      .catch(() => {
        toast.error('读取全局开关状态失败');
      });
    invoke<BurstRule[]>('get_rules')
      .then((loaded) => {
        if (loaded.length === 0) {
          pushRules(() => defaultRules());
        } else {
          setRules(loaded);
        }
      })
      .catch(() => {
        toast.error('读取规则失败，已加载默认配置');
        pushRules(() => defaultRules());
      });
  }, []);

  function persistCloseBehavior(v: CloseBehavior) {
    settingsStore
      .set(CLOSE_BEHAVIOR_KEY, v)
      .then(() => settingsStore.save())
      .catch(() => {
        toast.warning('保存关闭行为偏好失败');
      });
  }

  function toggleGlobal() {
    const next = !globalEnabled;
    setGlobalEnabled(next);
    invoke('set_global_enabled', { enabled: next }).catch(() => {
      toast.error('切换全局开关失败');
      setGlobalEnabled(!next);
    });
  }

  function pushRules(updater: (prev: BurstRule[]) => BurstRule[]) {
    setRules((prev) => {
      const next = updater(prev);
      queueMicrotask(() => {
        invoke('set_rules', { rules: next }).catch(() => {
          toast.error('保存规则失败');
        });
      });
      return next;
    });
  }

  function addRule(mode: BurstMode = 'hold') {
    pushRules((prev) => [...prev, newRule(mode)]);
  }

  function removeRule(id: string) {
    pushRules((prev) => prev.filter((r) => r.id !== id));
  }

  function updateRule(id: string, patch: Partial<BurstRule>) {
    pushRules((prev) =>
      prev.map((r) => {
        if (r.id !== id) return r;
        const merged = { ...r, ...patch };
        if (patch.trigger_key !== undefined && r.mode === 'hold' && !advancedOpen[id]) {
          merged.target_key = patch.trigger_key;
        }
        return merged;
      }),
    );
  }

  function toggleAdvanced(id: string) {
    setAdvancedOpen((s) => ({ ...s, [id]: !s[id] }));
  }

  async function handleDelete(id: string) {
    const ok = await confirm({
      title: '删除规则',
      description: '确认删除此规则？此操作不可撤销。',
      confirmText: '删除',
      tone: 'danger',
    });
    if (ok) removeRule(id);
  }

  async function handleRestoreDefaults() {
    const ok = await confirm({
      title: '恢复默认配置',
      description: '将清空当前所有规则并重置为默认配置，确认继续？',
      confirmText: '恢复默认',
      tone: 'danger',
    });
    if (ok) {
      pushRules(() => defaultRules());
      setAdvancedOpen({});
    }
  }

  function handleClose() {
    settingsStore
      .get<CloseBehavior>(CLOSE_BEHAVIOR_KEY)
      .then((remembered) => {
        if (remembered === 'exit') getCurrentWindow().destroy();
        else if (remembered === 'minimize') getCurrentWindow().hide();
        else void askCloseBehavior();
      })
      .catch(() => {
        toast.error('读取关闭行为偏好失败');
        void askCloseBehavior();
      });
  }

  async function askCloseBehavior() {
    const result: { choice: CloseBehavior; remember: boolean } = {
      choice: 'minimize',
      remember: false,
    };
    const ok = await confirm({
      title: '关闭窗口',
      description: '关闭按钮的行为：',
      confirmText: '确定',
      body: (
        <CloseBehaviorForm
          defaultChoice="minimize"
          onChange={(c, r) => {
            result.choice = c;
            result.remember = r;
          }}
        />
      ),
    });
    if (!ok) return;
    if (result.remember) persistCloseBehavior(result.choice);
    if (result.choice === 'exit') getCurrentWindow().destroy();
    else getCurrentWindow().hide();
  }

  return (
    <div
      className={`panel${globalEnabled ? ' on' : ' off'}`}
      style={{ ['--panel-bg' as string]: `url(${bgUrl})` }}
    >
      <header className="panel-header" data-tauri-drag-region>
        <img className="header-icon" src={iconUrl} alt="" data-tauri-drag-region />
        <h1 data-tauri-drag-region>气质花按键助手 v0.1</h1>
        <div className="window-controls">
          <button
            className="win-btn"
            onClick={() => getCurrentWindow().minimize()}
            aria-label="最小化"
          >
            ─
          </button>
          <button className="win-btn close" onClick={handleClose} aria-label="关闭">
            ✕
          </button>
        </div>
      </header>

      <section className="rules-section">
        <div className="tab-bar">
          {(['hold', 'toggle'] as BurstMode[]).map((mode) => {
            const groupRules = rules.filter((r) => r.mode === mode);
            const active = groupRules.filter((r) => r.enabled).length;
            const title = mode === 'hold' ? '按压连发' : '切换连发';
            return (
              <button
                key={mode}
                className={`tab${activeTab === mode ? ' active' : ''}`}
                onClick={() => {
                  setActiveTab(mode);
                  settingsStore
                    .set(ACTIVE_TAB_KEY, mode)
                    .then(() => settingsStore.save())
                    .catch(() => {
                      toast.warning('保存当前标签页失败');
                    });
                }}
              >
                <span className="tab-title">{title}</span>
                <span className="tab-count">
                  {active}/{groupRules.length}
                </span>
              </button>
            );
          })}
        </div>

        {(['hold', 'toggle'] as BurstMode[]).map((mode) => {
          if (mode !== activeTab) return null;
          const groupRules = rules.filter((r) => r.mode === mode);
          const title = mode === 'hold' ? '按压连发' : '切换连发';
          return (
            <div className="rule-group" key={mode}>
              <div className="rules-list">
                {groupRules.length === 0 && <p className="empty">暂无{title}规则</p>}
                {groupRules.map((rule) => (
                  <div key={rule.id} className={`rule-row${rule.enabled ? '' : ' disabled'}`}>
                    <button
                      className="del-btn"
                      onClick={() => handleDelete(rule.id)}
                      aria-label="删除"
                      title="删除"
                    >
                      ✕
                    </button>
                    <div className="rule-main">
                      <input
                        type="checkbox"
                        checked={rule.enabled}
                        onChange={(e) => updateRule(rule.id, { enabled: e.target.checked })}
                      />
                      {mode === 'hold' ? (
                        <>
                          <div className="rule-field">
                            <label>按压键</label>
                            <KeyCapture
                              value={rule.trigger_key}
                              onChange={(vk) => updateRule(rule.id, { trigger_key: vk })}
                            />
                          </div>
                          <span className="rule-arrow">→</span>
                          <div className="rule-field">
                            <label>连发按键</label>
                            <KeyCapture
                              value={rule.target_key}
                              onChange={(vk) => updateRule(rule.id, { target_key: vk })}
                            />
                          </div>
                        </>
                      ) : (
                        <>
                          <div className="rule-field">
                            <label>启动热键</label>
                            <KeyCapture
                              value={rule.trigger_key}
                              onChange={(vk) => updateRule(rule.id, { trigger_key: vk })}
                            />
                          </div>
                          <span className="rule-arrow">→</span>
                          <div className="rule-field">
                            <label>连发按键</label>
                            <KeyCapture
                              value={rule.target_key}
                              onChange={(vk) => updateRule(rule.id, { target_key: vk })}
                            />
                          </div>
                        </>
                      )}
                      <div className="rule-field rule-interval">
                        <label>间隔</label>
                        <div className="interval-input">
                          <input
                            type="number"
                            min={10}
                            max={10000}
                            value={rule.interval_ms}
                            onChange={(e) =>
                              updateRule(rule.id, {
                                interval_ms: Math.max(10, Math.min(10000, Number(e.target.value))),
                              })
                            }
                          />
                          <span>ms</span>
                        </div>
                      </div>
                    </div>
                    {mode === 'toggle' && advancedOpen[rule.id] && (
                      <div className="rule-advanced">
                        <div className="rule-field">
                          <label>停止热键</label>
                          <KeyCapture
                            value={rule.stop_key ?? rule.trigger_key}
                            onChange={(vk) => updateRule(rule.id, { stop_key: vk })}
                          />
                        </div>
                        <span className="adv-hint">默认与启动热键相同</span>
                      </div>
                    )}
                    {mode === 'toggle' && (
                      <button
                        className={`expand-btn${advancedOpen[rule.id] ? ' open' : ''}`}
                        onClick={() => toggleAdvanced(rule.id)}
                        aria-label="高级设置"
                      >
                        <svg
                          className="chevron"
                          viewBox="0 0 12 12"
                          width="10"
                          height="10"
                          aria-hidden="true"
                        >
                          <path
                            d="M2 4 L6 8 L10 4"
                            fill="none"
                            stroke="currentColor"
                            strokeWidth="1.6"
                            strokeLinecap="round"
                            strokeLinejoin="round"
                          />
                        </svg>
                        <span className="expand-label">
                          {advancedOpen[rule.id] ? '收起高级设置' : '高级设置'}
                        </span>
                      </button>
                    )}
                  </div>
                ))}
              </div>
              <button className="add-btn" onClick={() => addRule(mode)}>
                + 添加{title}规则
              </button>
            </div>
          );
        })}
      </section>

      <footer className="panel-footer">
        <button className="reset-btn" onClick={handleRestoreDefaults} title="恢复默认配置">
          恢复默认
        </button>
        <span className="footer-label">全局开关</span>
        <button className={`toggle-btn${globalEnabled ? ' active' : ''}`} onClick={toggleGlobal}>
          {globalEnabled ? '已启用' : '已禁用'}
        </button>
      </footer>
    </div>
  );
}

function CloseBehaviorForm({
  defaultChoice,
  onChange,
}: {
  defaultChoice: CloseBehavior;
  onChange: (choice: CloseBehavior, remember: boolean) => void;
}) {
  const [choice, setChoice] = useState<CloseBehavior>(defaultChoice);
  const [remember, setRemember] = useState(false);

  function update(c: CloseBehavior, r: boolean) {
    setChoice(c);
    setRemember(r);
    onChange(c, r);
  }

  return (
    <>
      <label className="radio-row">
        <input
          type="radio"
          name="close-choice"
          checked={choice === 'minimize'}
          onChange={() => update('minimize', remember)}
        />
        <span>
          <strong>最小化到托盘</strong>
          <small>程序继续在后台运行（推荐）</small>
        </span>
      </label>
      <label className="radio-row">
        <input
          type="radio"
          name="close-choice"
          checked={choice === 'exit'}
          onChange={() => update('exit', remember)}
        />
        <span>
          <strong>直接退出</strong>
          <small>关闭程序与所有连发功能</small>
        </span>
      </label>
      <label className="check-row">
        <input
          type="checkbox"
          checked={remember}
          onChange={(e) => update(choice, e.target.checked)}
        />
        <span>记住我的选择</span>
      </label>
    </>
  );
}
