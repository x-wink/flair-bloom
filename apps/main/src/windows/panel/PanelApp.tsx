import { getVersion } from '@tauri-apps/api/app';
import { APP_NAME } from '../../constants';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { LazyStore } from '@tauri-apps/plugin-store';
import { useCallback, useEffect, useRef, useState } from 'react';
import iconUrl from '../../assets/icon-32.png';
import bgUrl from '../../assets/icon.png';
import AboutDialog from './dialogs/AboutDialog';
import UpdateNoticeDialog, { type UpdateNoticeInfo } from './dialogs/UpdateNoticeDialog';
import AgreementDialog from './dialogs/AgreementDialog';
import CloseBehaviorForm, { type CloseBehavior } from './components/CloseBehaviorForm';
import ContextMenu from './components/ContextMenu';
import { ChevronIcon, CloseIcon, MenuIcon, MinimizeIcon } from './components/icons';
import KeyCapture from './components/KeyCapture';
import Overlay from './components/Overlay';
import { useConfirm } from './components/ConfirmDialog';
import { useToast } from './components/Toast';
import './PanelApp.css';

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
  const [driverMode, setDriverMode] = useState(false);
  const [driverInstalled, setDriverInstalled] = useState(false);
  const [togglingDriver, setTogglingDriver] = useState(false);
  const [rules, setRules] = useState<BurstRule[]>([]);
  const [profileName, setProfileName] = useState('默认配置');
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
      .then((mode) => setDriverMode(mode === 'interception'))
      .catch(() => {});
    invoke<boolean>('is_driver_installed')
      .then(setDriverInstalled)
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

  async function toggleDriverMode() {
    if (togglingDriver) return;
    setTogglingDriver(true);
    try {
      if (!driverMode) {
        if (!driverInstalled) {
          const ok = await confirm({
            title: '安装驱动',
            description: (
              <>
                驱动增强需要安装 Interception 内核驱动。点击「安装」后将弹出 UAC
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
                <strong>安装完成后请重启电脑</strong>，重启后再次开启驱动增强即可生效。
              </>
            ),
            confirmText: '我已知晓',
            cancelText: '稍后处理',
          });
          return;
        }
        await invoke('set_input_mode', { mode: 'interception' });
        const actual = await invoke<string>('get_input_mode');
        if (actual === 'interception') {
          setDriverMode(true);
          toast.success('驱动增强已启用');
        } else {
          toast.warning('驱动未就绪，请重启电脑后再试');
        }
      } else {
        await invoke('set_input_mode', { mode: 'sendinput' });
        setDriverMode(false);
        toast.info('已切换为标准模式');
      }
    } catch (e) {
      toast.error(`切换失败：${e}`);
    } finally {
      setTogglingDriver(false);
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
          将卸载 Interception 内核驱动。卸载后驱动增强功能将不可用，应用会切回标准模式。
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
      setDriverMode(false);
      setDriverInstalled(false);
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
                    <div className="rule-body">
                      <div className="rule-main">
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
                        <ChevronIcon size={10} className="chevron" />
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
        <div className="footer-controls">
          <div className="footer-control">
            <span className="footer-label">驱动增强</span>
            <button
              className={`toggle-btn mini${driverMode ? ' active' : ''}`}
              onClick={toggleDriverMode}
              disabled={togglingDriver}
              title="启用后可兼容剑网三等使用 Raw Input 的游戏"
            >
              {togglingDriver ? '…' : driverMode ? '开' : '关'}
            </button>
          </div>
          <div className="footer-control">
            <span className="footer-label">全局开关</span>
            <button
              className={`toggle-btn${globalEnabled ? ' active' : ''}`}
              onClick={toggleGlobal}
              disabled={togglingGlobal}
            >
              {togglingGlobal ? '切换中…' : globalEnabled ? '已启用' : '已禁用'}
            </button>
          </div>
        </div>
      </footer>

      <ContextMenu
        open={menuOpen}
        onClose={() => setMenuOpen(false)}
        target={menuBtnRef}
        items={[
          { label: '检查更新', onClick: handleCheckUpdate },
          {
            label: updateNotice ? '更新公告 ●' : '更新公告',
            onClick: handleShowUpdateNotice,
          },
          { label: '查看日志', onClick: handleOpenLogDir },
          { label: '用户协议', onClick: handleShowAgreement },
          { label: '关于', onClick: handleShowAbout },
          ...(driverInstalled
            ? [{ label: '卸载驱动', onClick: handleUninstallDriver, danger: true }]
            : []),
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
