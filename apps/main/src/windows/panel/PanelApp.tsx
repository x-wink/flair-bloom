import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { LazyStore } from '@tauri-apps/plugin-store';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import iconUrl from '../../assets/icon-32.png';
import bgUrl from '../../assets/icon.png';
import { APP_NAME } from '../../constants';
import CloseBehaviorForm, { type CloseBehavior } from './components/CloseBehaviorForm';
import { useConfirm } from './components/ConfirmDialog';
import ContextMenu, { type ContextMenuItem } from './components/ContextMenu';
import { ChevronIcon, CloseIcon, MenuIcon, MinimizeIcon } from './components/icons';
import KeyCapture, { keyboardKey, type KeyId } from './components/KeyCapture';
import { detectConflicts, severityForKey, severityForRule } from './conflicts';
import Overlay from './components/Overlay';
import ProfileNameForm from './components/ProfileNameForm';
import { useToast } from './components/Toast';
import UpdateProgressBar, { type UpdateDownloadProgress } from './components/UpdateProgressBar';
import Button from './components/Button';
import AboutDialog, { type AboutDialogInfo } from './dialogs/AboutDialog';
import AgreementDialog from './dialogs/AgreementDialog';
import ImportDialog from './dialogs/ImportDialog';
import RepairDialog from './dialogs/RepairDialog';
import UpdateNoticeDialog, { type UpdateNoticeInfo } from './dialogs/UpdateNoticeDialog';
import './PanelApp.css';

const settingsStore = new LazyStore('settings.json');
const CLOSE_BEHAVIOR_KEY = 'closeBehavior';
const ACTIVE_TAB_KEY = 'activeTab';
const DEFAULT_PROFILE_NAME = 'defults';

type BurstMode = 'hold' | 'toggle';
type InputMode = 'sendinput' | 'interception' | 'dd_hid';
type DriverStatus = 'installed' | 'pending_reboot' | 'not_installed';

interface AppStatus {
  elevated: boolean;
  interception_installed: DriverStatus;
  dd_hid_installed: DriverStatus;
  input_mode: string;
  platform: string;
  os_family: string;
  os_version: string;
  webview_version: string;
  arch: string;
  locale: string;
  install_path: string;
  log_dir: string;
  app_data_dir: string;
  autostart_enabled: boolean;
  resources_ok: boolean;
  missing_resources: string[];
}

const APP_STATUS_EVENT = 'app-status-changed';
const UPDATE_PROGRESS_EVENT = 'update-download-progress';
const UPDATE_FAILED_EVENT = 'update-download-failed';

interface UpdateDownloadFailed {
  version: string;
  message: string;
}

const INPUT_MODE_LABELS: Record<InputMode, string> = {
  sendinput: '通用模式',
  interception: '游戏模式',
  dd_hid: '究极HID',
};
const INPUT_MODE_LIST: InputMode[] = ['sendinput', 'interception', 'dd_hid'];

interface BurstRule {
  id: string;
  enabled: boolean;
  trigger_key: KeyId;
  target_key: KeyId;
  mode: BurstMode;
  stop_key: KeyId | null;
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
  hotkeys: { global_toggle: KeyId | null; global_stop?: KeyId | null; panel_toggle?: KeyId | null };
  advanced: { log_level: string };
}

interface ProfileEntry {
  meta: ProfileMeta;
  path: string;
}

interface ForkResult {
  profile: Profile;
  path: string;
}

function newRule(mode: BurstMode = 'hold'): BurstRule {
  const isHold = mode === 'hold';
  const vk = isHold ? 0x51 : 0x46;
  const key = keyboardKey(vk);
  return {
    id: crypto.randomUUID(),
    enabled: !isHold,
    trigger_key: key,
    target_key: key,
    mode,
    stop_key: null,
    interval_ms: 10,
  };
}

function defaultRules(): BurstRule[] {
  return [newRule('hold'), newRule('toggle')];
}

function buildProfileMenu(args: {
  profiles: ProfileEntry[];
  activeName: string;
  isDefault: boolean;
  onSwitch: (path: string) => void;
  onCreate: () => void;
  onImport: () => void;
  onRename: () => void;
  onDelete: () => void;
}): ContextMenuItem[] {
  const { profiles, activeName, isDefault, onSwitch, onCreate, onImport, onRename, onDelete } =
    args;
  const items: ContextMenuItem[] = profiles.map((p) => ({
    label: p.meta.name === DEFAULT_PROFILE_NAME ? '默认配置' : p.meta.name,
    subtitle: p.meta.name === DEFAULT_PROFILE_NAME ? '出厂预设，修改时自动新建' : undefined,
    active: p.meta.name === activeName,
    onClick: () => onSwitch(p.path),
  }));
  if (items.length > 0) items.push({ type: 'divider' });
  items.push({ label: '新建配置…', onClick: onCreate });
  items.push({ label: '导入外部配置…', onClick: onImport });
  items.push({
    label: '重命名当前配置…',
    disabled: isDefault,
    subtitle: isDefault ? '默认配置不可重命名' : undefined,
    onClick: onRename,
  });
  items.push({
    label: '删除当前配置',
    danger: true,
    disabled: isDefault,
    subtitle: isDefault ? '默认配置不可删除' : undefined,
    onClick: onDelete,
  });
  return items;
}

export default function PanelApp() {
  const [showAgreement, setShowAgreement] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [showAbout, setShowAbout] = useState(false);
  const [showRepair, setShowRepair] = useState(false);
  const [showImport, setShowImport] = useState(false);
  const [appVersion, setAppVersion] = useState('');
  const [updateNotice, setUpdateNotice] = useState<UpdateNoticeInfo | null>(null);
  const [showUpdateNotice, setShowUpdateNotice] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<UpdateDownloadProgress | null>(null);
  const menuBtnRef = useRef<HTMLButtonElement>(null);
  const [globalEnabled, setGlobalEnabled] = useState(false);
  const [togglingGlobal, setTogglingGlobal] = useState(false);
  const [inputMode, setInputMode] = useState<InputMode>('sendinput');
  const [interceptionInstalled, setInterceptionInstalled] = useState<DriverStatus>('not_installed');
  const [ddHidInstalled, setDdHidInstalled] = useState<DriverStatus>('not_installed');
  const [elevated, setElevated] = useState(false);
  const [sysInfo, setSysInfo] = useState<{
    platform: string;
    os_family: string;
    os_version: string;
    webview_version: string;
    arch: string;
    locale: string;
    install_path: string;
    log_dir: string;
    app_data_dir: string;
    autostart_enabled: boolean;
    resources_ok: boolean;
    missing_resources: string[];
  }>({
    platform: '',
    os_family: '',
    os_version: '',
    webview_version: '',
    arch: '',
    locale: '',
    install_path: '',
    log_dir: '',
    app_data_dir: '',
    autostart_enabled: false,
    resources_ok: true,
    missing_resources: [],
  });
  const [switchingMode, setSwitchingMode] = useState(false);
  const [modePickerOpen, setModePickerOpen] = useState(false);
  const modeBtnRef = useRef<HTMLButtonElement>(null);
  const [rules, setRules] = useState<BurstRule[]>([]);
  const [activeRuleIds, setActiveRuleIds] = useState<Set<string>>(new Set());
  const [profileName, setProfileName] = useState('defults');
  const [profileList, setProfileList] = useState<ProfileEntry[]>([]);
  const [profileMenuOpen, setProfileMenuOpen] = useState(false);
  const profileBtnRef = useRef<HTMLButtonElement>(null);
  const [advancedOpen, setAdvancedOpen] = useState<Record<string, boolean>>({});
  const [activeTab, setActiveTab] = useState<BurstMode>('toggle');
  const [hotkeys, setHotkeys] = useState<{
    global_toggle: KeyId | null;
    global_stop: KeyId | null;
    panel_toggle: KeyId | null;
  }>({ global_toggle: null, global_stop: null, panel_toggle: null });
  const hotkeysRef = useRef(hotkeys);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const conflicts = useMemo(() => detectConflicts(rules, hotkeys), [rules, hotkeys]);
  const confirm = useConfirm();
  const toast = useToast();
  const saveTimer = useRef<ReturnType<typeof setTimeout>>();
  const updateProgressDoneTimer = useRef<ReturnType<typeof setTimeout>>();
  const updateDownloadFailedRef = useRef(false);
  const initialLoadDone = useRef(false);
  const profileNameRef = useRef(profileName);
  const isDefaultProfile = profileName === DEFAULT_PROFILE_NAME;

  const applyAppStatus = useCallback((status: AppStatus) => {
    setElevated(status.elevated);
    setInterceptionInstalled(status.interception_installed);
    setDdHidInstalled(status.dd_hid_installed);
    setSysInfo({
      platform: status.platform,
      os_family: status.os_family,
      os_version: status.os_version,
      webview_version: status.webview_version,
      arch: status.arch,
      locale: status.locale,
      install_path: status.install_path,
      log_dir: status.log_dir,
      app_data_dir: status.app_data_dir,
      autostart_enabled: status.autostart_enabled,
      resources_ok: status.resources_ok,
      missing_resources: status.missing_resources,
    });
    if ((INPUT_MODE_LIST as string[]).includes(status.input_mode)) {
      setInputMode(status.input_mode as InputMode);
    }
  }, []);

  useEffect(() => {
    return () => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
      if (updateProgressDoneTimer.current) clearTimeout(updateProgressDoneTimer.current);
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

    invoke<AppStatus>('get_app_status')
      .then(applyAppStatus)
      .catch(() => {});

    // 启动期：以「activeProfilePath → load_profile」为唯一来源；
    // 没路径或加载失败则回退到 init_default_profile。
    void (async () => {
      const refreshList = async () => {
        try {
          const list = await invoke<ProfileEntry[]>('list_profiles');
          setProfileList(list);
        } catch {
          /* 启动时静默失败，后续操作再提示 */
        }
      };
      try {
        const activePath = await invoke<string | null>('get_active_profile_path');
        if (activePath) {
          try {
            const profile = await invoke<Profile>('load_profile', { path: activePath });
            setRules(profile.rules);
            setHotkeys({
              global_toggle: profile.hotkeys.global_toggle ?? null,
              global_stop: profile.hotkeys.global_stop ?? null,
              panel_toggle: profile.hotkeys.panel_toggle ?? null,
            });
            setProfileName(profile.meta.name);
            await refreshList();
            queueMicrotask(() => {
              initialLoadDone.current = true;
            });
            return;
          } catch {
            toast.warning('加载配置失败，已切换为默认配置');
          }
        }
        const profile = await invoke<Profile>('init_default_profile');
        setRules(profile.rules);
        setHotkeys({
          global_toggle: profile.hotkeys.global_toggle ?? null,
          global_stop: profile.hotkeys.global_stop ?? null,
          panel_toggle: profile.hotkeys.panel_toggle ?? null,
        });
        setProfileName(profile.meta.name);
        await refreshList();
      } catch {
        toast.error('初始化默认配置失败');
        setRules(defaultRules());
        invoke('set_rules', { rules: defaultRules() }).catch(() => {});
      } finally {
        queueMicrotask(() => {
          initialLoadDone.current = true;
        });
      }
    })();

    const unlistenAgreement = listen<string>('show-agreement', () => {
      setShowAgreement(true);
    });
    // 兜底：如果 emit 在 listen 注册前就已触发（WebView 加载慢时可能丢失事件）
    invoke<boolean>('needs_agreement')
      .then((needed) => {
        if (needed) setShowAgreement(true);
      })
      .catch(() => {});
    const unlistenStatus = listen<AppStatus>(APP_STATUS_EVENT, (e) => {
      applyAppStatus(e.payload);
    });
    const unlistenGlobal = listen<boolean>('global-enabled-changed', (e) => {
      setGlobalEnabled(e.payload);
    });
    const unlistenDownloading = listen<string>('update-downloading', (e) => {
      if (updateProgressDoneTimer.current) clearTimeout(updateProgressDoneTimer.current);
      updateDownloadFailedRef.current = false;
      setUpdateProgress({
        version: e.payload,
        downloaded: 0,
        total: null,
        percent: null,
        done: false,
      });
      toast.info(`发现新版本 v${e.payload}，正在下载更新…`);
    });
    const unlistenProgress = listen<UpdateDownloadProgress>(UPDATE_PROGRESS_EVENT, (e) => {
      if (updateProgressDoneTimer.current) clearTimeout(updateProgressDoneTimer.current);
      setUpdateProgress(e.payload);
      if (e.payload.done) {
        updateProgressDoneTimer.current = setTimeout(() => {
          setUpdateProgress((current) =>
            current?.version === e.payload.version && current.done ? null : current,
          );
        }, 1800);
      }
    });
    const unlistenFailed = listen<UpdateDownloadFailed>(UPDATE_FAILED_EVENT, (e) => {
      if (updateProgressDoneTimer.current) clearTimeout(updateProgressDoneTimer.current);
      updateDownloadFailedRef.current = true;
      setUpdateProgress(null);
      toast.warning(`下载更新失败：${e.payload.message}`);
    });
    const unlistenReady = listen<UpdateNoticeInfo>('update-ready', (e) => {
      setUpdateNotice(e.payload);
      setShowUpdateNotice(true);
    });
    const unlistenUpToDate = listen('update-not-available', () => {
      setUpdateProgress(null);
      toast.info('已是最新版本');
    });
    const unlistenClose = getCurrentWindow().onCloseRequested((event) => {
      event.preventDefault();
      void handleClose();
    });
    return () => {
      unlistenAgreement.then((fn) => fn());
      unlistenStatus.then((fn) => fn());
      unlistenGlobal.then((fn) => fn());
      unlistenDownloading.then((fn) => fn());
      unlistenProgress.then((fn) => fn());
      unlistenFailed.then((fn) => fn());
      unlistenReady.then((fn) => fn());
      unlistenUpToDate.then((fn) => fn());
      unlistenClose.then((fn) => fn());
    };
  }, []);

  // 规则/热键变更后防抖自动保存到 .qzh
  profileNameRef.current = profileName;
  hotkeysRef.current = hotkeys;

  const refreshProfileList = useCallback(async () => {
    try {
      const list = await invoke<ProfileEntry[]>('list_profiles');
      setProfileList(list);
    } catch {
      toast.warning('读取配置列表失败');
    }
  }, [toast]);

  // forkPromiseRef：正在进行的 fork_active_profile 调用；
  // saveProfile 在写盘前 await 它，确保竞态时写入目标是已 fork 的配置。
  const forkPromiseRef = useRef<Promise<void> | null>(null);

  // ensureWritableProfile：若当前是默认配置，启动 fork；已在 fork 中则返回同一 Promise。
  // 只能由用户操作（pushRules / 热键 onChange）调用，不能放进会在 profile load 时触发的 effect。
  function ensureWritableProfile(): Promise<void> {
    if (profileNameRef.current !== DEFAULT_PROFILE_NAME) return Promise.resolve();
    if (forkPromiseRef.current) return forkPromiseRef.current;
    if (saveTimer.current) {
      clearTimeout(saveTimer.current);
      saveTimer.current = undefined;
    }
    forkPromiseRef.current = invoke<ForkResult>('fork_active_profile', {
      suggestedName: '我的配置',
    })
      .then(async (res) => {
        setProfileName(res.profile.meta.name);
        profileNameRef.current = res.profile.meta.name;
        await refreshProfileList();
        toast.success(`已为你创建新配置「${res.profile.meta.name}」`);
      })
      .catch((e) => {
        toast.error(`创建新配置失败：${e}`);
      })
      .finally(() => {
        forkPromiseRef.current = null;
      });
    return forkPromiseRef.current;
  }

  // saveProfile：防抖 500ms，写盘前 await 任何正在进行的 fork，保证写到正确配置。
  const saveProfile = useCallback((r: BurstRule[], hk: typeof hotkeys) => {
    if (saveTimer.current) clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(async () => {
      await (forkPromiseRef.current ?? Promise.resolve());
      const name = profileNameRef.current;
      const profile: Profile = {
        schema_version: 2,
        meta: { name, created_at: 0, updated_at: 0, app_version: '' },
        rules: r,
        hotkeys: {
          global_toggle: hk.global_toggle,
          global_stop: hk.global_stop,
          panel_toggle: hk.panel_toggle,
        },
        advanced: { log_level: 'info' },
      };
      invoke('save_profile', { name, profile }).catch(() => {
        toast.warning('自动保存配置失败');
      });
    }, 500);
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // 规则变更时自动保存（跳过初始加载，避免启动时重复写入）
  useEffect(() => {
    if (!initialLoadDone.current) return;
    saveProfile(rules, hotkeysRef.current);
  }, [rules, saveProfile]);

  // 热键变更时：立即通知引擎 + 写盘（saveProfile 内部会等待正在进行的 fork）
  useEffect(() => {
    if (!initialLoadDone.current) return;
    invoke('set_global_hotkeys', {
      hotkeys: {
        global_toggle: hotkeys.global_toggle,
        global_stop: hotkeys.global_stop,
        panel_toggle: hotkeys.panel_toggle,
      },
    }).catch(() => {});
    saveProfile(rules, hotkeys);
  }, [hotkeys]); // eslint-disable-line react-hooks/exhaustive-deps

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

  // 全局开关切换时播报语音；initialLoadDone 为 true 后才响应，跳过启动阶段的状态同步
  useEffect(() => {
    if (!initialLoadDone.current) return;
    if (!('speechSynthesis' in window)) return;
    window.speechSynthesis.cancel();
    const utt = new SpeechSynthesisUtterance(globalEnabled ? '我准备好库库按了' : '我累了歇会');
    utt.lang = 'zh-CN';
    utt.rate = 1.15;
    window.speechSynthesis.speak(utt);
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
      if (target === 'interception' && interceptionInstalled !== 'installed') {
        if (interceptionInstalled === 'pending_reboot') {
          await confirm({
            title: '请重启电脑',
            description: (
              <>
                检测到游戏模式驱动处于「待重启」状态——上次安装或卸载尚未完成，
                必须重启电脑后才能切换到此模式。
              </>
            ),
            confirmText: '我已知晓',
            cancelText: '稍后处理',
          });
          return;
        }
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
      if (target === 'dd_hid' && ddHidInstalled !== 'installed') {
        if (ddHidInstalled === 'pending_reboot') {
          await confirm({
            title: '请重启电脑',
            description: (
              <>
                检测到究极HID 驱动处于「待重启」状态——上次卸载尚未由 PnP 完成清理。
                请重启电脑后再尝试切换到此模式。
              </>
            ),
            confirmText: '我已知晓',
            cancelText: '稍后处理',
          });
          return;
        }
        const ok = await confirm({
          title: '安装究极HID 驱动',
          description: (
            <>
              究极HID 模式需要安装 ddxoft 提供的 WHQL 签名 HID 虚拟驱动。点击「安装」后将弹出 UAC
              授权窗口，授权后会出现一个一闪而过的命令行窗口即为安装完成。
              <br />
              <br />
              <strong>通常不需要重启即可生效。</strong>
            </>
          ),
          confirmText: '安装',
        });
        if (!ok) return;
        const installResult = await invoke<{ pending_reboot: boolean }>('install_dd_hid_driver');
        if (installResult.pending_reboot) {
          await confirm({
            title: '安装完成，建议重启电脑',
            description: (
              <>
                究极HID 驱动已安装，但 Windows PnP 报告驱动文件已更新，建议重启电脑以确保完全生效。
                <br />
                <br />
                驱动在重启前通常已可正常工作，可尝试直接切换；若遇到异常请重启后再试。
              </>
            ),
            confirmText: '我已知晓',
            cancelText: '稍后重启',
          });
        } else {
          toast.success('究极HID 驱动已安装');
        }
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
      const status = await invoke<AppStatus>('get_app_status');
      applyAppStatus(status);
      const actual = status.input_mode;
      if (actual === target) {
        toast.success(`已切换为${INPUT_MODE_LABELS[target]}`);
      } else if ((INPUT_MODE_LIST as string[]).includes(actual)) {
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
    // 用户编辑规则时启动 fork（若在默认配置）；写盘由 rules effect 里的 saveProfile 负责，
    // saveProfile 内部会 await forkPromiseRef，确保写到 fork 后的新配置。
    void ensureWritableProfile();
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

  async function switchToProfile(path: string) {
    if (saveTimer.current) {
      clearTimeout(saveTimer.current);
      saveTimer.current = undefined;
    }
    initialLoadDone.current = false;
    try {
      const profile = await invoke<Profile>('load_profile', { path });
      setRules(profile.rules);
      setHotkeys({
        global_toggle: profile.hotkeys.global_toggle ?? null,
        global_stop: profile.hotkeys.global_stop ?? null,
        panel_toggle: profile.hotkeys.panel_toggle ?? null,
      });
      setProfileName(profile.meta.name);
      profileNameRef.current = profile.meta.name;
      setAdvancedOpen({});
      await refreshProfileList();
    } catch (e) {
      toast.error(`切换配置失败：${e}`);
    } finally {
      queueMicrotask(() => {
        initialLoadDone.current = true;
      });
    }
  }

  async function handleCreateProfile() {
    let name = '';
    const ok = await confirm({
      title: '新建配置',
      description: '请输入新配置的名称：',
      confirmText: '创建',
      body: (
        <ProfileNameForm
          placeholder="例如：刺客 / 法师 / 测试用"
          onChange={(v) => {
            name = v;
          }}
        />
      ),
    });
    if (!ok) return;
    const trimmed = name.trim();
    if (!trimmed) {
      toast.warning('配置名不能为空');
      return;
    }
    if (trimmed === DEFAULT_PROFILE_NAME) {
      toast.warning('不能使用默认配置名');
      return;
    }
    if (profileList.some((p) => p.meta.name === trimmed)) {
      toast.warning('已存在同名配置');
      return;
    }
    // 复用 fork：会基于「当前激活配置」创建副本，名字冲突由后端 pick_unique_name 兜底
    try {
      const res = await invoke<ForkResult>('fork_active_profile', { suggestedName: trimmed });
      setRules(res.profile.rules);
      setProfileName(res.profile.meta.name);
      profileNameRef.current = res.profile.meta.name;
      setAdvancedOpen({});
      await refreshProfileList();
      toast.success(`已创建配置「${res.profile.meta.name}」`);
    } catch (e) {
      toast.error(`创建失败：${e}`);
    }
  }

  async function handleRenameProfile() {
    if (isDefaultProfile) {
      toast.warning('默认配置不可重命名');
      return;
    }
    let name = profileName;
    const ok = await confirm({
      title: '重命名配置',
      description: `当前配置「${profileName}」的新名称：`,
      confirmText: '重命名',
      body: (
        <ProfileNameForm
          defaultValue={profileName}
          onChange={(v) => {
            name = v;
          }}
        />
      ),
    });
    if (!ok) return;
    const trimmed = name.trim();
    if (!trimmed || trimmed === profileName) return;
    if (trimmed === DEFAULT_PROFILE_NAME) {
      toast.warning('不能使用默认配置名');
      return;
    }
    try {
      await invoke<string>('rename_profile', {
        oldName: profileName,
        newName: trimmed,
      });
      setProfileName(trimmed);
      profileNameRef.current = trimmed;
      await refreshProfileList();
      toast.success(`已重命名为「${trimmed}」`);
    } catch (e) {
      toast.error(`重命名失败：${e}`);
    }
  }

  async function handleDeleteProfile() {
    if (isDefaultProfile) {
      toast.warning('默认配置不可删除');
      return;
    }
    const ok = await confirm({
      title: '删除配置',
      description: `确定删除配置「${profileName}」？删除后将自动切换为默认配置。`,
      confirmText: '删除',
      tone: 'danger',
    });
    if (!ok) return;
    if (saveTimer.current) {
      clearTimeout(saveTimer.current);
      saveTimer.current = undefined;
    }
    initialLoadDone.current = false;
    try {
      const fallback = await invoke<Profile | null>('delete_profile', { name: profileName });
      if (fallback) {
        setRules(fallback.rules);
        setHotkeys({
          global_toggle: fallback.hotkeys.global_toggle ?? null,
          global_stop: fallback.hotkeys.global_stop ?? null,
          panel_toggle: fallback.hotkeys.panel_toggle ?? null,
        });
        setProfileName(fallback.meta.name);
        profileNameRef.current = fallback.meta.name;
        setAdvancedOpen({});
      }
      await refreshProfileList();
      toast.success('配置已删除');
    } catch (e) {
      toast.error(`删除失败：${e}`);
    } finally {
      queueMicrotask(() => {
        initialLoadDone.current = true;
      });
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

  function handleShowAgreement() {
    setMenuOpen(false);
    setShowAgreement(true);
  }

  function handleCheckUpdate() {
    setMenuOpen(false);
    updateDownloadFailedRef.current = false;
    invoke('check_update').catch(() => {
      setUpdateProgress(null);
      if (!updateDownloadFailedRef.current) {
        toast.warning('检查更新失败，请检查网络连接后重试');
      }
    });
  }

  function handleShowUpdateNotice() {
    setMenuOpen(false);
    if (updateNotice) {
      setShowUpdateNotice(true);
    } else {
      updateDownloadFailedRef.current = false;
      invoke('check_update').catch(() => {
        setUpdateProgress(null);
        if (!updateDownloadFailedRef.current) {
          toast.warning('检查更新失败，请检查网络连接后重试');
        }
      });
    }
  }

  function handleShowAbout() {
    setMenuOpen(false);
    setShowAbout(true);
  }

  function handleShowRepair() {
    setMenuOpen(false);
    setShowRepair(true);
  }

  async function handleUninstallDriver() {
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
    const ok = await confirm({
      title: '卸载究极HID 驱动',
      description: (
        <>
          将卸载究极HID 虚拟驱动。卸载后究极HID 模式将不可用，应用会切回通用模式。
          <br />
          <br />
          卸载流程会调用 PnP 标准接口处理，建议卸载完成后重启电脑再尝试重新安装。
        </>
      ),
      confirmText: '卸载',
      cancelText: '取消',
      tone: 'danger',
    });
    if (!ok) return;
    try {
      const r = await invoke<{ message: string; pending_reboot: boolean }>(
        'uninstall_dd_hid_driver',
      );
      if (r.pending_reboot) {
        toast.warning(r.message, 8000);
      } else {
        toast.success(r.message || '究极HID 驱动已卸载');
      }
    } catch (e) {
      toast.error(`卸载失败：${e}`);
    }
  }

  function handleAgreed() {
    setShowAgreement(false);
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
      {updateProgress && <UpdateProgressBar progress={updateProgress} />}

      <section className="rules-section">
        {/* 热键设置区：属于规则配置，跟着配置文件切换 */}
        <div className="hotkey-section">
          {/* 全局开关 */}
          <div className="hotkey-item">
            <span className="hotkey-item-label">全局开关</span>
            <div className="hotkey-item-keys">
              <KeyCapture
                value={hotkeys.global_toggle}
                onChange={(k) => {
                  void ensureWritableProfile();
                  setHotkeys((prev) => ({
                    ...prev,
                    global_toggle: k,
                    global_stop: k === null ? null : prev.global_stop,
                  }));
                }}
                nullable
                placeholder="未设置"
                conflict={severityForKey(conflicts, hotkeys.global_toggle)}
              />
              {/* 停止键仅在开启键已设置时显示。
                  导入时若来源配置只有 stop 无 toggle，gaibang_parse_hotkeys 也不会产生
                  stop-only 结构（因为 toggle==0 时 global_toggle=None 而 global_stop 不会被单独保留），
                  故此处无需额外处理 stop-only 场景。 */}
              {hotkeys.global_toggle && (
                <>
                  <span className="hotkey-item-sep">停止</span>
                  <KeyCapture
                    value={hotkeys.global_stop}
                    onChange={(k) => {
                      void ensureWritableProfile();
                      setHotkeys((prev) => ({ ...prev, global_stop: k }));
                    }}
                    nullable
                    placeholder="同开启键"
                    conflict={severityForKey(conflicts, hotkeys.global_stop)}
                  />
                </>
              )}
            </div>
          </div>

          {/* 面板显隐 */}
          <div className="hotkey-item">
            <span className="hotkey-item-label">面板显隐</span>
            <div className="hotkey-item-keys">
              <KeyCapture
                value={hotkeys.panel_toggle}
                onChange={(k) => {
                  void ensureWritableProfile();
                  setHotkeys((prev) => ({ ...prev, panel_toggle: k }));
                }}
                nullable
                placeholder="未设置"
                conflict={severityForKey(conflicts, hotkeys.panel_toggle)}
              />
            </div>
          </div>
        </div>

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
                                  if (!vk) return;
                                  const patch: Partial<BurstRule> = { target_key: vk };
                                  // 高级未展开时，触发键跟随连发键同步，符合「等技能 CD 好就按」的默认场景
                                  if (!showAdvanced) patch.trigger_key = vk;
                                  updateRule(rule.id, patch);
                                }}
                                conflict={
                                  !showAdvanced ? severityForRule(conflicts, rule.id) : null
                                }
                              />
                            </div>
                          ) : (
                            <>
                              <div className="rule-field">
                                <label>启动热键</label>
                                <KeyCapture
                                  value={rule.trigger_key}
                                  onChange={(vk) => vk && updateRule(rule.id, { trigger_key: vk })}
                                  conflict={severityForRule(conflicts, rule.id)}
                                />
                              </div>
                              <span className="rule-arrow">→</span>
                              <div className="rule-field">
                                <label>连发按键</label>
                                <KeyCapture
                                  value={rule.target_key}
                                  onChange={(vk) => vk && updateRule(rule.id, { target_key: vk })}
                                  conflict={severityForRule(conflicts, rule.id)}
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
                              onChange={(vk) => vk && updateRule(rule.id, { trigger_key: vk })}
                              conflict={severityForRule(conflicts, rule.id)}
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
                              onChange={(vk) => vk && updateRule(rule.id, { stop_key: vk })}
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
          ref={profileBtnRef}
          variant="outline"
          tone="neutral"
          size="sm"
          appendIcon={<ChevronIcon size={10} />}
          onClick={() => setProfileMenuOpen((v) => !v)}
          title={isDefaultProfile ? '默认配置（修改后将自动新建）' : `当前配置：${profileName}`}
        >
          {isDefaultProfile ? '默认配置' : profileName}
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
        open={profileMenuOpen}
        onClose={() => setProfileMenuOpen(false)}
        target={profileBtnRef}
        location="bottom-left"
        items={buildProfileMenu({
          profiles: profileList,
          activeName: profileName,
          isDefault: isDefaultProfile,
          onSwitch: switchToProfile,
          onCreate: handleCreateProfile,
          onImport: () => {
            setProfileMenuOpen(false);
            setShowImport(true);
          },
          onRename: handleRenameProfile,
          onDelete: handleDeleteProfile,
        })}
      />

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
                ? interceptionInstalled === 'installed'
                  ? '兼容多数游戏'
                  : interceptionInstalled === 'pending_reboot'
                    ? '驱动待重启生效'
                    : '点击安装驱动'
                : ddHidInstalled === 'installed'
                  ? '极致兼容，HVCI 友好'
                  : ddHidInstalled === 'pending_reboot'
                    ? '驱动待重启清理'
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
          { label: '用户协议', onClick: handleShowAgreement },
          { label: '环境修复', onClick: handleShowRepair },
          { label: '关于', onClick: handleShowAbout },
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
        <AboutDialog
          info={
            {
              appVersion,
              elevated,
              interception_installed: interceptionInstalled,
              dd_hid_installed: ddHidInstalled,
              input_mode: inputMode,
              platform: sysInfo.platform,
              os_family: sysInfo.os_family,
              os_version: sysInfo.os_version,
              webview_version: sysInfo.webview_version,
              arch: sysInfo.arch,
              locale: sysInfo.locale,
              install_path: sysInfo.install_path,
              log_dir: sysInfo.log_dir,
              app_data_dir: sysInfo.app_data_dir,
              autostart_enabled: sysInfo.autostart_enabled,
              resources_ok: sysInfo.resources_ok,
              missing_resources: sysInfo.missing_resources,
              global_enabled: globalEnabled,
              rules_total: rules.length,
              rules_enabled: rules.filter((r) => r.enabled).length,
              active_rule_ids: Array.from(activeRuleIds),
            } satisfies AboutDialogInfo
          }
          onClose={() => setShowAbout(false)}
          onUninstallInterception={handleUninstallDriver}
          onUninstallDdHid={handleUninstallDdHid}
          onOpenDir={(kind) => {
            invoke('open_app_dir', { kind }).catch((err) => {
              toast.warning(`打开目录失败：${err}`);
            });
          }}
          onCopied={() => toast.success('已复制状态信息')}
          onCopyFailed={(e) => toast.error(`复制失败：${e}`)}
        />
      </Overlay>

      <Overlay open={showRepair} onClose={() => setShowRepair(false)}>
        <RepairDialog
          onClose={() => setShowRepair(false)}
          onToast={(kind, msg) => {
            if (kind === 'success') toast.success(msg);
            else if (kind === 'warn') toast.warning(msg);
            else toast.error(msg);
          }}
        />
      </Overlay>

      <Overlay open={showImport} onClose={() => setShowImport(false)}>
        <ImportDialog
          onClose={() => setShowImport(false)}
          onImported={async (name) => {
            setShowImport(false);
            // 导入后以新配置名重新加载（后端已切换 activeProfilePath）
            initialLoadDone.current = false;
            try {
              const activePath = await invoke<string | null>('get_active_profile_path');
              if (activePath) {
                const profile = await invoke<Profile>('load_profile', { path: activePath });
                setRules(profile.rules);
                setHotkeys({
                  global_toggle: profile.hotkeys.global_toggle ?? null,
                  global_stop: profile.hotkeys.global_stop ?? null,
                  panel_toggle: profile.hotkeys.panel_toggle ?? null,
                });
                setProfileName(profile.meta.name);
                profileNameRef.current = profile.meta.name;
                setAdvancedOpen({});
                await refreshProfileList();
                toast.success(`已导入配置「${name}」`);
              }
            } catch (e) {
              toast.error(`导入后加载失败：${e}`);
            } finally {
              queueMicrotask(() => {
                initialLoadDone.current = true;
              });
            }
          }}
        />
      </Overlay>
    </div>
  );
}
