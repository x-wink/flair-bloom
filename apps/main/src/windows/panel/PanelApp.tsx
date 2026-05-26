import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { LazyStore } from '@tauri-apps/plugin-store';
import { useCallback, useEffect, useRef, useState } from 'react';
import iconUrl from '../../assets/icon-32.png';
import bgUrl from '../../assets/icon.png';
import { APP_NAME } from '../../constants';
import CloseBehaviorForm, { type CloseBehavior } from './components/CloseBehaviorForm';
import { useConfirm } from './components/ConfirmDialog';
import ContextMenu from './components/ContextMenu';
import { ChevronIcon, CloseIcon, MenuIcon, MinimizeIcon } from './components/icons';
import KeyCapture from './components/KeyCapture';
import Overlay from './components/Overlay';
import { useToast } from './components/Toast';
import Button from './components/Button';
import AboutDialog from './dialogs/AboutDialog';
import AgreementDialog from './dialogs/AgreementDialog';
import UpdateNoticeDialog, { type UpdateNoticeInfo } from './dialogs/UpdateNoticeDialog';
import './PanelApp.css';

const settingsStore = new LazyStore('settings.json');
const CLOSE_BEHAVIOR_KEY = 'closeBehavior';
const ACTIVE_TAB_KEY = 'activeTab';

type BurstMode = 'hold' | 'toggle';
type InputMode = 'sendinput' | 'interception' | 'dd_hid';

const INPUT_MODE_LABELS: Record<InputMode, string> = {
  sendinput: '通用模式',
  interception: '游戏模式',
  dd_hid: '究极HID',
};
const INPUT_MODE_LIST: InputMode[] = ['sendinput', 'interception', 'dd_hid'];

interface BurstRule {
  id: string;
  enabled: boolean;
  trigger_key: number;
  target_key: number;
  mode: BurstMode;
  stop_key: number | null;
  interval_ms: number;
}

interface ProfileMeta {
  name: string;
  created_at: number;
  updated_at: number;
  app_version: string;
}

interface Profile {
  schema_version: number;
  meta: ProfileMeta;
  rules: BurstRule[];
  hotkeys: { global_toggle: number | null };
  advanced: { log_level: string };
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

export default function PanelApp() {
  const [showAgreement, setShowAgreement] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [showAbout, setShowAbout] = useState(false);
  const [appVersion, setAppVersion] = useState('');
  const [updateNotice, setUpdateNotice] = useState<UpdateNoticeInfo | null>(null);
  const [showUpdateNotice, setShowUpdateNotice] = useState(false);
  const menuBtnRef = useRef<HTMLButtonElement>(null);
  const [globalEnabled, setGlobalEnabled] = useState(false);
  const [togglingGlobal, setTogglingGlobal] = useState(false);
  const [inputMode, setInputMode] = useState<InputMode>('sendinput');
  const [interceptionInstalled, setInterceptionInstalled] = useState(false);
  const [ddHidInstalled, setDdHidInstalled] = useState(false);
  const [elevated, setElevated] = useState(false);
  const [switchingMode, setSwitchingMode] = useState(false);
  const [modePickerOpen, setModePickerOpen] = useState(false);
  const modeBtnRef = useRef<HTMLButtonElement>(null);
  const [rules, setRules] = useState<BurstRule[]>([]);
  const [activeRuleIds, setActiveRuleIds] = useState<Set<string>>(new Set());
  const [profileName, setProfileName] = useState('defults');
  const [advancedOpen, setAdvancedOpen] = useState<Record<string, boolean>>({});
  const [activeTab, setActiveTab] = useState<BurstMode>('toggle');
  const confirm = useConfirm();
  const toast = useToast();
  const saveTimer = useRef<ReturnType<typeof setTimeout>>();
  const initialLoadDone = useRef(false);
  const profileNameRef = useRef(profileName);

  useEffect(() => {
    return () => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
    };
  }, []);

  useEffect(() => {
    getVersion()
      .then(setAppVersion)
      .catch(() => {});
  }, []);

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

    invoke<string>('get_input_mode')
      .then((mode) => {
        if ((INPUT_MODE_LIST as string[]).includes(mode)) {
          setInputMode(mode as InputMode);
        }
      })
      .catch(() => {});
    invoke<boolean>('is_driver_installed')
      .then(setInterceptionInstalled)
      .catch(() => {});
    invoke<boolean>('is_dd_hid_driver_installed')
      .then(setDdHidInstalled)
      .catch(() => {});
    invoke<boolean>('is_elevated')
      .then(setElevated)
      .catch(() => {});

    // 引擎已在启动时从 .qzh 加载了规则，直接读取
    invoke<BurstRule[]>('get_rules')
      .then((loaded) => {
        if (loaded.length === 0) {
          // 首次启动，初始化默认配置
          invoke<Profile>('init_default_profile')
            .then((profile) => {
              setRules(profile.rules);
              setProfileName(profile.meta.name);
              queueMicrotask(() => {
                initialLoadDone.current = true;
              });
            })
            .catch(() => {
              toast.error('初始化默认配置失败');
              setRules(defaultRules());
              invoke('set_rules', { rules: defaultRules() }).catch(() => {});
              queueMicrotask(() => {
                initialLoadDone.current = true;
              });
            });
        } else {
          setRules(loaded);
          queueMicrotask(() => {
            initialLoadDone.current = true;
          });
        }
      })
      .catch(() => {
        toast.error('读取规则失败，已加载默认配置');
        setRules(defaultRules());
        invoke('set_rules', { rules: defaultRules() }).catch(() => {});
        queueMicrotask(() => {
          initialLoadDone.current = true;
        });
      });

    const unlistenAgreement = listen<string>('show-agreement', () => {
      setShowAgreement(true);
    });
    // 兜底：如果 emit 在 listen 注册前就已触发（WebView 加载慢时可能丢失事件）
    invoke<boolean>('needs_agreement')
      .then((needed) => {
        if (needed) setShowAgreement(true);
      })
      .catch(() => {});
    const unlistenGlobal = listen<boolean>('global-enabled-changed', (e) => {
      setGlobalEnabled(e.payload);
    });
    const unlistenDownloading = listen<string>('update-downloading', (e) => {
      toast.info(`发现新版本 v${e.payload}，正在下载更新…`);
    });
    const unlistenReady = listen<UpdateNoticeInfo>('update-ready', (e) => {
      setUpdateNotice(e.payload);
      setShowUpdateNotice(true);
    });
    const unlistenUpToDate = listen('update-not-available', () => {
      toast.info('已是最新版本');
    });
    const unlistenClose = getCurrentWindow().onCloseRequested((event) => {
      event.preventDefault();
      void handleClose();
    });
    return () => {
      unlistenAgreement.then((fn) => fn());
      unlistenGlobal.then((fn) => fn());
      unlistenDownloading.then((fn) => fn());
      unlistenReady.then((fn) => fn());
      unlistenUpToDate.then((fn) => fn());
      unlistenClose.then((fn) => fn());
    };
  }, []);

  // 规则变更后防抖自动保存到 .qzh
  profileNameRef.current = profileName;
  const saveRules = useCallback((r: BurstRule[]) => {
    if (saveTimer.current) clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => {
      const name = profileNameRef.current;
      const profile: Profile = {
        schema_version: 1,
        meta: {
          name,
          created_at: 0, // backend will set timestamps
          updated_at: 0,
          app_version: '',
        },
        rules: r,
        hotkeys: { global_toggle: null },
        advanced: { log_level: 'info' },
      };
      invoke('save_profile', { name, profile }).catch(() => {
        toast.warning('自动保存配置失败');
      });
    }, 500);
  }, []);

  // 规则变更时自动保存（跳过初始加载，避免启动时重复写入）
  useEffect(() => {
    if (!initialLoadDone.current) return;
    saveRules(rules);
  }, [rules, saveRules]);

  // 全局开关启用时轮询活动规则 ID，驱动激活态脉冲动画。
  // 关闭时清空，避免残留高亮。
  useEffect(() => {
    if (!globalEnabled) {
      setActiveRuleIds((prev) => (prev.size === 0 ? prev : new Set()));
      return;
    }
    let cancelled = false;
    const poll = () => {
      invoke<string[]>('get_active_rules')
        .then((ids) => {
          if (cancelled) return;
          setActiveRuleIds((prev) => {
            if (prev.size === ids.length && ids.every((id) => prev.has(id))) return prev;
            return new Set(ids);
          });
        })
        .catch(() => {});
    };
    poll();
    const timer = setInterval(poll, 120);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [globalEnabled]);

  function persistCloseBehavior(v: CloseBehavior) {
    settingsStore
      .set(CLOSE_BEHAVIOR_KEY, v)
      .then(() => settingsStore.save())
      .catch(() => {
        toast.warning('保存关闭行为偏好失败');
      });
  }

  async function toggleGlobal() {
    if (togglingGlobal) return;
    const next = !globalEnabled;
    setTogglingGlobal(true);
    try {
      await invoke('set_global_enabled', { enabled: next });
      setGlobalEnabled(next);
    } catch {
      toast.error('切换全局开关失败');
    } finally {
      setTogglingGlobal(false);
    }
  }

  async function selectInputMode(target: InputMode) {
    if (switchingMode || target === inputMode) return;
    setSwitchingMode(true);
    try {
      // Interception 驱动：未安装则先安装并退出（要求重启电脑）
      if (target === 'interception' && !interceptionInstalled) {
        const ok = await confirm({
          title: '安装驱动',
          description: (
            <>
              游戏模式需要安装 Interception 内核驱动。点击「安装」后将弹出 UAC
              授权窗口，授权后控制台窗口会一闪而过即为安装完成。
              <br />
              <br />
              <strong>安装完成后必须重启电脑，驱动才会生效。</strong>
            </>
          ),
          confirmText: '安装',
        });
        if (!ok) return;
        await invoke('install_driver');
        await confirm({
          title: '请重启电脑',
          description: (
            <>
              驱动安装程序已启动。如系统弹出「可能未正确安装此程序」提示，请点击「已正确安装此程序」。
              <br />
              <br />
              <strong>安装完成后请重启电脑</strong>，重启后再次切换到游戏模式即可生效。
            </>
          ),
          confirmText: '我已知晓',
          cancelText: '稍后处理',
        });
        return;
      }

      // DD-HID：未安装则先 PnP 安装（无需重启）
      if (target === 'dd_hid' && !ddHidInstalled) {
        const ok = await confirm({
          title: '安装究极HID 驱动',
          description: (
            <>
              究极HID 模式需要安装 ddxoft 提供的 WHQL 签名 HID 虚拟驱动。点击「安装」后将弹出 UAC
              授权窗口，授权后会出现一个一闪而过的命令行窗口即为安装完成。
              <br />
              <br />
              <strong>本驱动无需重启电脑即可生效。</strong>
            </>
          ),
          confirmText: '安装',
        });
        if (!ok) return;
        await invoke('install_dd_hid_driver');
        setDdHidInstalled(true);
        toast.success('究极HID 驱动已安装');
      }

      // DD-HID 模式需要管理员：当前非管理员则提示重启
      const targetNeedsAdmin = target === 'dd_hid';
      if (targetNeedsAdmin && !elevated) {
        const ok = await confirm({
          title: '需要管理员权限',
          description: (
            <>
              究极HID 模式底层调用 DeviceIoControl，需要以管理员身份运行。
              <br />
              <br />
              点击「以管理员重启」会立刻关闭当前应用并启动管理员实例，自动切换到所选模式。
            </>
          ),
          confirmText: '以管理员重启',
        });
        if (!ok) return;
        try {
          await invoke('relaunch_as_admin', { mode: target });
        } catch (e) {
          toast.error(`提权重启失败：${e}`);
        }
        return; // 进程将退出，不再继续
      }

      // 常规切换
      await invoke('set_input_mode', { mode: target });
      const actual = await invoke<string>('get_input_mode');
      if (actual === target) {
        setInputMode(target);
        toast.success(`已切换为${INPUT_MODE_LABELS[target]}`);
      } else if ((INPUT_MODE_LIST as string[]).includes(actual)) {
        setInputMode(actual as InputMode);
        toast.warning(
          target === 'interception'
            ? '驱动未就绪，请重启电脑后再试'
            : `切换未生效，已停留在${INPUT_MODE_LABELS[actual as InputMode]}`,
        );
      }
    } catch (e) {
      toast.error(`切换失败：${e}`);
    } finally {
      setSwitchingMode(false);
    }
  }

  function pushRules(updater: (prev: BurstRule[]) => BurstRule[]) {
    const next = updater(rules);
    setRules(next);
    invoke('set_rules', { rules: next }).catch(async (e) => {
      toast.error(`保存规则失败：${e}`);
      try {
        const engineRules = await invoke<BurstRule[]>('get_rules');
        setRules(engineRules);
      } catch {
        setRules(defaultRules());
      }
    });
  }

  function addRule(mode: BurstMode = 'hold') {
    pushRules((prev) => [...prev, newRule(mode)]);
  }

  function removeRule(id: string) {
    pushRules((prev) => prev.filter((r) => r.id !== id));
  }

  function updateRule(id: string, patch: Partial<BurstRule>) {
    pushRules((prev) => prev.map((r) => (r.id === id ? { ...r, ...patch } : r)));
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

  function handleOpenLogDir() {
    setMenuOpen(false);
    invoke('open_log_dir').catch(() => {
      toast.warning('打开日志文件夹失败');
    });
  }

  function handleShowAgreement() {
    setMenuOpen(false);
    setShowAgreement(true);
  }

  function handleCheckUpdate() {
    setMenuOpen(false);
    invoke('check_update').catch(() => {
      toast.warning('检查更新失败，请检查网络连接后重试');
    });
  }

  function handleShowUpdateNotice() {
    setMenuOpen(false);
    if (updateNotice) {
      setShowUpdateNotice(true);
    } else {
      invoke('check_update').catch(() => {
        toast.warning('检查更新失败，请检查网络连接后重试');
      });
    }
  }

  function handleShowAbout() {
    setMenuOpen(false);
    setShowAbout(true);
  }

  async function handleUninstallDriver() {
    setMenuOpen(false);
    const ok = await confirm({
      title: '卸载驱动',
      description: (
        <>
          将卸载 Interception 内核驱动。卸载后游戏模式将不可用，应用会切回通用模式。
          <br />
          <br />
          <strong>卸载完成后必须重启电脑才能彻底生效。</strong>
        </>
      ),
      confirmText: '卸载',
      cancelText: '取消',
      tone: 'danger',
    });
    if (!ok) return;
    try {
      await invoke('uninstall_driver');
      if (inputMode === 'interception') setInputMode('sendinput');
      setInterceptionInstalled(false);
      await confirm({
        title: '卸载完成',
        description: (
          <>
            驱动已卸载。如系统弹出「可能未正确安装此程序」提示，请点击「已正确安装此程序」。
            <br />
            <br />
            <strong>请重启电脑使卸载彻底生效。</strong>
          </>
        ),
        confirmText: '我已知晓',
        cancelText: '稍后处理',
      });
    } catch (e) {
      toast.error(`卸载失败：${e}`);
    }
  }

  async function handleUninstallDdHid() {
    setMenuOpen(false);
    const ok = await confirm({
      title: '卸载究极HID 驱动',
      description: (
        <>
          将卸载究极HID 虚拟驱动。卸载后究极HID 模式将不可用，应用会切回通用模式。
          <br />
          <br />
          本驱动卸载无需重启电脑。
        </>
      ),
      confirmText: '卸载',
      cancelText: '取消',
      tone: 'danger',
    });
    if (!ok) return;
    try {
      await invoke('uninstall_dd_hid_driver');
      if (inputMode === 'dd_hid') setInputMode('sendinput');
      setDdHidInstalled(false);
      toast.success('究极HID 驱动已卸载');
    } catch (e) {
      toast.error(`卸载失败：${e}`);
    }
  }

  function handleAgreed() {
    invoke<BurstRule[]>('get_rules').then((loaded) => {
      if (loaded.length === 0) {
        invoke<Profile>('init_default_profile')
          .then((profile) => {
            setRules(profile.rules);
            setProfileName(profile.meta.name);
          })
          .catch(() => {
            setRules(defaultRules());
            invoke('set_rules', { rules: defaultRules() }).catch(() => {});
          });
      } else {
        setRules(loaded);
      }
      setShowAgreement(false);
    });
  }

  return (
    <div
      className={`panel${globalEnabled ? ' on' : ' off'}`}
      style={{ ['--panel-bg' as string]: `url(${bgUrl})` }}
    >
      <header className="panel-header" data-tauri-drag-region>
        <img className="header-icon" src={iconUrl} alt="" data-tauri-drag-region />
        <h1 data-tauri-drag-region>
          {APP_NAME}
          {appVersion ? ` v${appVersion}` : ''}
        </h1>
        <div className="window-controls">
          <button
            ref={menuBtnRef}
            className="win-btn menu-btn"
            onClick={() => setMenuOpen((v) => !v)}
            aria-label="菜单"
          >
            <MenuIcon size={14} />
          </button>
          <button
            className="win-btn"
            onClick={() => getCurrentWindow().minimize()}
            aria-label="最小化"
          >
            <MinimizeIcon size={14} />
          </button>
          <button className="win-btn close" onClick={handleClose} aria-label="关闭">
            <CloseIcon size={14} />
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
                {groupRules.map((rule) => {
                  const isActive = activeRuleIds.has(rule.id);
                  const showAdvanced = advancedOpen[rule.id];
                  return (
                    <div
                      key={rule.id}
                      className={`rule-row${rule.enabled ? '' : ' disabled'}${isActive ? ' active' : ''}`}
                    >
                      <button
                        className="del-btn"
                        onClick={() => handleDelete(rule.id)}
                        aria-label="删除"
                        title="删除"
                      >
                        ✕
                      </button>
                      <div className="rule-body">
                        <div className="rule-main">
                          {mode === 'hold' ? (
                            <div className="rule-field">
                              <label>连发按键</label>
                              <KeyCapture
                                value={rule.target_key}
                                onChange={(vk) => {
                                  const patch: Partial<BurstRule> = { target_key: vk };
                                  // 高级未展开时，触发键跟随连发键同步，符合「等技能 CD 好就按」的默认场景
                                  if (!showAdvanced) patch.trigger_key = vk;
                                  updateRule(rule.id, patch);
                                }}
                              />
                            </div>
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
                                    interval_ms: Math.max(
                                      10,
                                      Math.min(10000, Number(e.target.value)),
                                    ),
                                  })
                                }
                              />
                              <span>ms</span>
                            </div>
                          </div>
                        </div>
                        <input
                          type="checkbox"
                          className="enable-checkbox"
                          checked={rule.enabled}
                          onChange={(e) => updateRule(rule.id, { enabled: e.target.checked })}
                          aria-label="启用"
                        />
                      </div>
                      {mode === 'hold' && showAdvanced && (
                        <div className="rule-advanced">
                          <div className="rule-field">
                            <label>按压键</label>
                            <KeyCapture
                              value={rule.trigger_key}
                              onChange={(vk) => updateRule(rule.id, { trigger_key: vk })}
                            />
                          </div>
                          <span className="adv-hint">默认与连发按键相同</span>
                        </div>
                      )}
                      {mode === 'toggle' && showAdvanced && (
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
                      <button
                        className={`expand-btn${showAdvanced ? ' open' : ''}`}
                        onClick={() => toggleAdvanced(rule.id)}
                        aria-label="高级设置"
                      >
                        <ChevronIcon size={10} className="chevron" />
                        <span className="expand-label">
                          {showAdvanced ? '收起高级设置' : '高级设置'}
                        </span>
                      </button>
                    </div>
                  );
                })}
              </div>
              <Button
                className="add-btn"
                variant="dashed"
                tone="primary"
                block
                onClick={() => addRule(mode)}
              >
                + 添加{title}规则
              </Button>
            </div>
          );
        })}
      </section>

      <footer className="panel-footer">
        <Button
          variant="outline"
          tone="neutral"
          size="sm"
          onClick={handleRestoreDefaults}
          title="恢复默认配置"
        >
          恢复默认
        </Button>
        <div className="footer-controls">
          <div className="footer-control">
            <span className="footer-label">输入模式</span>
            <Button
              ref={modeBtnRef}
              variant="outline"
              tone={inputMode !== 'sendinput' ? 'primary' : 'neutral'}
              size="sm"
              loading={switchingMode}
              appendIcon={<ChevronIcon size={10} />}
              onClick={() => setModePickerOpen((v) => !v)}
              title="点击选择输入模式"
            >
              {INPUT_MODE_LABELS[inputMode]}
              {elevated && inputMode === 'dd_hid' ? ' ★' : ''}
            </Button>
          </div>
          <div className="footer-control">
            <span className="footer-label">全局开关</span>
            <Button
              variant="solid"
              tone={globalEnabled ? 'primary' : 'neutral'}
              size="sm"
              loading={togglingGlobal}
              onClick={toggleGlobal}
            >
              {globalEnabled ? '已启用' : '已禁用'}
            </Button>
          </div>
        </div>
      </footer>

      <ContextMenu
        open={modePickerOpen}
        onClose={() => setModePickerOpen(false)}
        target={modeBtnRef}
        location="bottom-left"
        items={INPUT_MODE_LIST.map((m) => ({
          label: INPUT_MODE_LABELS[m],
          subtitle:
            m === 'sendinput'
              ? '最稳定，但很多游戏不响应'
              : m === 'interception'
                ? interceptionInstalled
                  ? '兼容多数游戏'
                  : '点击安装驱动'
                : ddHidInstalled
                  ? '极致兼容，HVCI 友好'
                  : '点击安装驱动',
          active: inputMode === m,
          onClick: () => void selectInputMode(m),
        }))}
      />

      <ContextMenu
        open={menuOpen}
        onClose={() => setMenuOpen(false)}
        target={menuBtnRef}
        items={[
          { label: '检查更新', onClick: handleCheckUpdate },
          {
            label: '更新公告',
            appendIcon: updateNotice ? (
              <span
                aria-hidden="true"
                style={{ width: 6, height: 6, borderRadius: '50%', background: '#6c4de6' }}
              />
            ) : undefined,
            onClick: handleShowUpdateNotice,
          },
          { label: '查看日志', onClick: handleOpenLogDir },
          { label: '用户协议', onClick: handleShowAgreement },
          { label: '关于', onClick: handleShowAbout },
          { type: 'divider' },
          {
            label: '卸载驱动',
            danger: true,
            disabled: !interceptionInstalled && !ddHidInstalled,
            children: [
              {
                label: '游戏模式驱动',
                onClick: handleUninstallDriver,
                danger: true,
                disabled: !interceptionInstalled,
              },
              {
                label: '究极HID 驱动',
                onClick: handleUninstallDdHid,
                danger: true,
                disabled: !ddHidInstalled,
              },
            ],
          },
        ]}
      />

      <Overlay open={showAgreement} onClose={() => setShowAgreement(false)} closeOnBackdrop={false}>
        <AgreementDialog onAgreed={handleAgreed} />
      </Overlay>

      <Overlay open={showUpdateNotice} onClose={() => setShowUpdateNotice(false)}>
        {updateNotice && (
          <UpdateNoticeDialog info={updateNotice} onClose={() => setShowUpdateNotice(false)} />
        )}
      </Overlay>

      <Overlay open={showAbout} onClose={() => setShowAbout(false)}>
        <AboutDialog version={appVersion} onClose={() => setShowAbout(false)} />
      </Overlay>
    </div>
  );
}
