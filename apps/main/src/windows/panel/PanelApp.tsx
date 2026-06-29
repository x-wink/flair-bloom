import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { LogicalSize } from '@tauri-apps/api/dpi';
import { emit, listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { LazyStore } from '@tauri-apps/plugin-store';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import iconUrl from '../../assets/icon-32.png';
import bgUrl from '../../assets/icon.png';
import { APP_NAME } from '../../constants';
import HorizontalLayout from './HorizontalLayout';
import Button from './components/Button';
import CloseBehaviorForm, { type CloseBehavior } from './components/CloseBehaviorForm';
import { useConfirm } from './components/ConfirmDialog';
import ContextMenu, { type ContextMenuItem } from './components/ContextMenu';
import { ChevronIcon, CloseIcon, EditIcon, MenuIcon, MinimizeIcon } from './components/icons';
import Kbd from './components/Kbd';
import KeyCapture, { keyboardKey, keyEq, keyLabel, type KeyId } from './components/KeyCapture';
import Overlay from './components/Overlay';
import ProfileNameForm from './components/ProfileNameForm';
import Tabs from './components/Tabs';
import { useToast } from './components/Toast';
import UpdateProgressBar, { type UpdateDownloadProgress } from './components/UpdateProgressBar';
import { detectConflicts, severityForKey, severityForRule } from './conflicts';
import { useKeyRelay } from './useKeyRelay';
import AboutDialog, { type AboutDialogInfo } from './dialogs/AboutDialog';
import AgreementDialog from './dialogs/AgreementDialog';
import ImportDialog from './dialogs/ImportDialog';
import RepairDialog from './dialogs/RepairDialog';
import SettingsDialog, { type SettingsTab, type SoundSettings } from './dialogs/SettingsDialog';
import {
  applyThemeColor,
  applyThemeMode,
  DEFAULT_THEME_COLOR,
  DEFAULT_THEME_MODE,
  SECT_PRESETS,
  type ThemeMode,
  type ThemeSettings,
} from './theme';
import UpdateNoticeDialog, { type UpdateNoticeInfo } from './dialogs/UpdateNoticeDialog';
import './PanelApp.css';

const settingsStore = new LazyStore('settings.json');
const CLOSE_BEHAVIOR_KEY = 'closeBehavior';
const ACTIVE_TAB_KEY = 'activeTab';
const SOUND_KEY = 'sound';
const THEME_KEY = 'theme';
const LAYOUT_KEY = 'layout';

// 面板布局：竖版规则列表 / 横版键鼠图。两者各自定尺寸，切换时 setSize + center。
type PanelLayout = 'vertical' | 'horizontal';
const VERTICAL_SIZE = { width: 405, height: 720 };
const HORIZONTAL_SIZE = { width: 1060, height: 580 };

const DEFAULT_THEME: ThemeSettings = {
  color: DEFAULT_THEME_COLOR,
  mode: DEFAULT_THEME_MODE,
};

const DEFAULT_SOUND: SoundSettings = {
  enabled: false,
  startEnabled: true,
  endEnabled: true,
  toggleStartEnabled: false,
  toggleEndEnabled: false,
  volume: 80,
  rate: 0,
  pitch: 0,
  startText: '我准备好库库按了',
  endText: '我累了歇会',
  toggleStartText: '${key}开始',
  toggleEndText: '停止${key}',
  voiceName: '',
  globalOnly: false,
};
const DEFAULT_PROFILE_NAME = 'defaults';
// 注入周期基础下限 10ms（≈100 taps/s）：管线每事件过路税决定可持续「总」注入速率。单规则用
// 此值；多条规则同时连发时后端按活跃规则数等分总速率（见 burst-engine process_due），避免叠加
// 超发导致停止「收不住」。与后端 MIN_EFFECTIVE_INTERVAL_MS 对齐。
const MIN_INTERVAL_MS = 10;
const DEFAULT_INTERVAL_MS = 10;
const MAX_INTERVAL_MS = 10000;

type BurstMode = 'hold' | 'toggle';
type InputMode = 'sendinput' | 'interception' | 'ddsimple' | 'dd_hid';
type DriverStatus = 'installed' | 'pending_reboot' | 'not_installed';

interface AppStatus {
  elevated: boolean;
  interception_installed: DriverStatus;
  dd_hid_installed: DriverStatus;
  input_mode: string;
  configured_input_mode: string;
  scheduler_hp_degraded: boolean;
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
  ddsimple: 'DD驱动',
  dd_hid: 'DDHID',
};
const INPUT_MODE_LIST: InputMode[] = ['sendinput', 'interception', 'ddsimple', 'dd_hid'];
function isInputMode(value: string): value is InputMode {
  return (INPUT_MODE_LIST as string[]).includes(value);
}

function inputModeRequiresAdmin(mode: InputMode): boolean {
  return mode !== 'sendinput';
}

// DD 系列驱动（DDSimple / DDHID）。横版键鼠图无法表达其单键规则约束，二者互斥。
function isDdInputMode(mode: InputMode): boolean {
  return mode === 'ddsimple' || mode === 'dd_hid';
}

const DD_HID_BLOCKED_NOTICE = (
  <>
    经测试 DDHID
    驱动不稳定，可能导致电脑蓝屏死机，现已禁用。需待新版本发布并通过内测验证稳定后，才可能重新开放。
    <br />
    <br />
    建议以管理员模式运行本应用、改用「游戏模式」；并在「诊断修复」中卸载 DDHID
    驱动，避免造成不良影响。
  </>
);

interface BurstRule {
  id: string;
  enabled: boolean;
  trigger_key: KeyId;
  target_key: KeyId;
  mode: BurstMode;
  stop_key: KeyId | null;
  interval_ms: number;
  group: string | null;
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

interface ProfileSummary {
  rules_total: number;
  rules_enabled: number;
  hold_count: number;
  toggle_count: number;
  group_count: number;
  global_toggle: KeyId | null;
  global_stop: KeyId | null;
  panel_toggle: KeyId | null;
}

interface ProfileEntry {
  meta: ProfileMeta;
  path: string;
  summary: ProfileSummary;
}

interface ForkResult {
  profile: Profile;
  path: string;
}

function newRule(mode: BurstMode = 'hold'): BurstRule {
  const isHold = mode === 'hold';
  const triggerVk = isHold ? 0x51 : 0x46; // Hold: Q, Toggle: F
  const targetVk = isHold ? 0x51 : 0x51; // Hold: Q, Toggle: Q（与 F 不同，避免 DD-HID 同键限制）
  return {
    id: crypto.randomUUID(),
    enabled: !isHold,
    trigger_key: keyboardKey(triggerVk),
    target_key: keyboardKey(targetVk),
    mode,
    stop_key: null,
    interval_ms: DEFAULT_INTERVAL_MS,
    group: null,
  };
}

function defaultRules(): BurstRule[] {
  return [newRule('hold'), newRule('toggle')];
}

/** 横版单键规则：触发键 == 连发键（== 停止键，toggle 时）。 */
function newSingleKeyRule(key: KeyId, mode: BurstMode, interval: number): BurstRule {
  return {
    id: crypto.randomUUID(),
    enabled: true,
    trigger_key: key,
    target_key: key,
    mode,
    stop_key: mode === 'toggle' ? key : null,
    interval_ms: interval,
    group: null,
  };
}

function buildProfileMenu(args: {
  profiles: ProfileEntry[];
  activeName: string;
  onSwitch: (path: string) => void;
  onCreate: () => void;
  onManage: () => void;
}): ContextMenuItem[] {
  const { profiles, activeName, onSwitch, onCreate, onManage } = args;
  const items: ContextMenuItem[] = profiles.map((p) => ({
    label: p.meta.name === DEFAULT_PROFILE_NAME ? '默认配置' : p.meta.name,
    subtitle: p.meta.name === DEFAULT_PROFILE_NAME ? '出厂预设，修改时自动新建' : undefined,
    active: p.meta.name === activeName,
    onClick: () => onSwitch(p.path),
  }));
  if (items.length > 0) items.push({ type: 'divider' });
  items.push({ label: '新建配置…', onClick: onCreate });
  items.push({ label: '管理配置…', subtitle: '重命名、删除、导入', onClick: onManage });
  return items;
}

function orderProfiles(profiles: ProfileEntry[]): ProfileEntry[] {
  return [...profiles].sort((a, b) => {
    const aDefault = a.meta.name === DEFAULT_PROFILE_NAME;
    const bDefault = b.meta.name === DEFAULT_PROFILE_NAME;
    if (aDefault !== bDefault) return aDefault ? -1 : 1;
    if (a.meta.created_at !== b.meta.created_at) return a.meta.created_at - b.meta.created_at;
    return a.meta.name.localeCompare(b.meta.name, 'zh-Hans-CN');
  });
}

export default function PanelApp() {
  const [showAgreement, setShowAgreement] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [settingsInitialTab, setSettingsInitialTab] = useState<SettingsTab>('general');
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
  const [closeBehaviorPreference, setCloseBehaviorPreference] = useState<CloseBehavior | null>(
    null,
  );
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
    scheduler_hp_degraded: boolean;
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
    scheduler_hp_degraded: false,
  });
  const [togglingAutostart, setTogglingAutostart] = useState(false);
  const [sound, setSound] = useState<SoundSettings>(DEFAULT_SOUND);
  const soundRef = useRef<SoundSettings>(DEFAULT_SOUND);
  const [theme, setTheme] = useState<ThemeSettings>(DEFAULT_THEME);
  const themeModeRef = useRef<ThemeMode>(DEFAULT_THEME.mode);
  const [availableVoices, setAvailableVoices] = useState<string[]>([]);
  const [switchingMode, setSwitchingMode] = useState(false);
  const [modePickerOpen, setModePickerOpen] = useState(false);
  const modeBtnRef = useRef<HTMLButtonElement>(null);
  const [rules, setRules] = useState<BurstRule[]>([]);
  const [activeRuleIds, setActiveRuleIds] = useState<Set<string>>(new Set());
  const prevActiveRuleIdsRef = useRef<Set<string>>(new Set());
  const [profileName, setProfileName] = useState('defaults');
  const [profileList, setProfileList] = useState<ProfileEntry[]>([]);
  const [profileMenuOpen, setProfileMenuOpen] = useState(false);
  const profileBtnRef = useRef<HTMLButtonElement>(null);
  const [advancedOpen, setAdvancedOpen] = useState<Record<string, boolean>>({});
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const draggingIdRef = useRef<string | null>(null);
  const [dragOverInfo, setDragOverInfo] = useState<
    | { kind: 'rule'; ruleId: string }
    | { kind: 'group'; name: string }
    | { kind: 'ungrouped' }
    | null
  >(null);
  const [pendingGroupName, setPendingGroupName] = useState<string | null>(null);
  const [editingGroupName, setEditingGroupName] = useState<{
    current: string;
    draft: string;
  } | null>(null);
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set());
  const [activeTab, setActiveTab] = useState<BurstMode>('toggle');
  const [layout, setLayout] = useState<PanelLayout>('vertical');
  // 横版统一间隔的回退值（无规则时显示用；有规则时显示取自规则本身）
  const [unifiedInterval, setUnifiedInterval] = useState(DEFAULT_INTERVAL_MS);
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
  const saveTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const updateProgressDoneTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const updateDownloadFailedRef = useRef(false);
  const initialLoadDone = useRef(false);
  const startupInputModeHandledRef = useRef(false);
  const ddHidFallbackInFlightRef = useRef(false);
  const ddHidNoticeOpenRef = useRef(false);
  const profileNameRef = useRef(profileName);
  // WebView2 聚焦时全局键盘钩子失效，将键盘事件中继到后端引擎（与浮窗共用）。
  useKeyRelay();
  const isDefaultProfile = profileName === DEFAULT_PROFILE_NAME;

  const showDdHidBlockedNotice = useCallback(async () => {
    if (ddHidNoticeOpenRef.current) return;
    ddHidNoticeOpenRef.current = true;
    try {
      await confirm({
        title: 'DDHID 已禁用',
        description: DD_HID_BLOCKED_NOTICE,
        confirmText: '知道了',
        cancelText: null,
      });
    } finally {
      ddHidNoticeOpenRef.current = false;
    }
  }, [confirm]);

  const applyAppStatus = useCallback(
    (status: AppStatus) => {
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
        scheduler_hp_degraded: status.scheduler_hp_degraded,
      });
      if (isInputMode(status.input_mode)) {
        if (status.input_mode === 'dd_hid') {
          setInputMode('sendinput');
          void showDdHidBlockedNotice();
          if (!ddHidFallbackInFlightRef.current) {
            ddHidFallbackInFlightRef.current = true;
            invoke('set_input_mode', { mode: 'sendinput' })
              .then(() => invoke<AppStatus>('get_app_status'))
              .then((nextStatus) => {
                if (nextStatus.input_mode !== 'dd_hid' && isInputMode(nextStatus.input_mode)) {
                  setInputMode(nextStatus.input_mode);
                }
              })
              .catch((e) => {
                toast.warning(`已屏蔽DDHID，但回退通用模式失败：${e}`);
              })
              .finally(() => {
                ddHidFallbackInFlightRef.current = false;
              });
          }
          return;
        }
        setInputMode(status.input_mode);
      }
    },
    [showDdHidBlockedNotice, toast],
  );

  async function reconcileStartupInputMode(status: AppStatus) {
    if (startupInputModeHandledRef.current) return;
    if (!isInputMode(status.configured_input_mode)) return;

    const configured = status.configured_input_mode;
    if (configured === 'sendinput' || configured === status.input_mode) return;

    startupInputModeHandledRef.current = true;

    if (configured === 'dd_hid') {
      await showDdHidBlockedNotice();
      try {
        await invoke('set_input_mode', { mode: 'sendinput' });
        const nextStatus = await invoke<AppStatus>('get_app_status');
        applyAppStatus(nextStatus);
      } catch (e) {
        toast.warning(`已屏蔽DDHID，但回退通用模式失败：${e}`);
      }
      return;
    }

    if (inputModeRequiresAdmin(configured) && !status.elevated) {
      const label = INPUT_MODE_LABELS[configured];
      const ok = await confirm({
        title: `以管理员模式恢复${label}`,
        description: (
          <>
            已保存的输入模式是{label}，需要管理员权限才能初始化驱动通道。当前将先使用通用模式；
            授权后应用会以管理员权限重启，并自动切换到{label}。
          </>
        ),
        confirmText: '以管理员重启',
        cancelText: '保持通用模式',
      });
      if (!ok) return;
      try {
        await invoke('relaunch_as_admin', { mode: configured });
      } catch (e) {
        toast.error(`启动管理员实例失败：${e}`);
      }
      return;
    }

    try {
      await invoke('set_input_mode', { mode: configured });
      const nextStatus = await invoke<AppStatus>('get_app_status');
      applyAppStatus(nextStatus);
      if (nextStatus.input_mode === configured) {
        toast.success(`已恢复为${INPUT_MODE_LABELS[configured]}`);
      } else if (isInputMode(nextStatus.input_mode)) {
        toast.warning(`未能恢复配置模式，已停留在${INPUT_MODE_LABELS[nextStatus.input_mode]}`);
      }
    } catch (e) {
      toast.error(`恢复配置模式失败：${e}`);
    }
  }

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

  // 主题：从 store 加载主色 + 亮暗模式并立即应用；mode=system 时监听系统配色变化。
  useEffect(() => {
    settingsStore
      .get<ThemeSettings>(THEME_KEY)
      .then((v) => {
        const merged: ThemeSettings = { ...DEFAULT_THEME, ...v };
        setTheme(merged);
        themeModeRef.current = merged.mode;
        applyThemeColor(merged.color);
        applyThemeMode(merged.mode);
      })
      .catch(() => {});

    const mql = window.matchMedia('(prefers-color-scheme: dark)');
    const onSystemChange = () => {
      if (themeModeRef.current === 'system') applyThemeMode('system');
    };
    mql.addEventListener('change', onSystemChange);
    return () => mql.removeEventListener('change', onSystemChange);
  }, []);

  // 声音设置：从 store 加载，同时枚举系统语音列表
  useEffect(() => {
    settingsStore
      .get<SoundSettings>(SOUND_KEY)
      .then((v) => {
        if (v && typeof v === 'object') {
          const merged = { ...DEFAULT_SOUND, ...v };
          setSound(merged);
          soundRef.current = merged;
        }
      })
      .catch(() => {});

    if (!('speechSynthesis' in window)) return;

    const speechSynthesis = window.speechSynthesis;
    const loadVoices = () => {
      const voices = speechSynthesis
        .getVoices()
        .filter((v) => v.lang.startsWith('zh') || v.lang.startsWith('en'))
        .map((v) => v.name);
      if (voices.length > 0) setAvailableVoices(voices);
    };
    loadVoices();

    if (
      typeof speechSynthesis.addEventListener === 'function' &&
      typeof speechSynthesis.removeEventListener === 'function'
    ) {
      speechSynthesis.addEventListener('voiceschanged', loadVoices);
      return () => speechSynthesis.removeEventListener('voiceschanged', loadVoices);
    }

    const previousVoicesChanged = speechSynthesis.onvoiceschanged;
    speechSynthesis.onvoiceschanged = (event) => {
      previousVoicesChanged?.call(speechSynthesis, event);
      loadVoices();
    };
    return () => {
      speechSynthesis.onvoiceschanged = previousVoicesChanged;
    };
  }, []);

  useEffect(() => {
    settingsStore
      .get<BurstMode>(ACTIVE_TAB_KEY)
      .then((v) => {
        if (v === 'hold' || v === 'toggle') setActiveTab(v);
      })
      .catch(() => {});

    settingsStore
      .get<CloseBehavior>(CLOSE_BEHAVIOR_KEY)
      .then((v) => {
        setCloseBehaviorPreference(v === 'exit' || v === 'minimize' ? v : null);
      })
      .catch(() => {});

    settingsStore
      .get<PanelLayout>(LAYOUT_KEY)
      .then((v) => {
        if (v === 'vertical' || v === 'horizontal') {
          setLayout(v);
          void applyWindowSize(v);
        }
      })
      .catch(() => {});

    invoke<boolean>('get_global_enabled')
      .then(setGlobalEnabled)
      .catch(() => {
        toast.error('读取全局开关状态失败');
      });

    invoke<AppStatus>('get_app_status')
      .then((status) => {
        applyAppStatus(status);
        void reconcileStartupInputMode(status);
      })
      .catch(() => {});

    // 启动期：以「activeProfilePath → load_profile」为唯一来源；
    // 没路径或加载失败则回退到 init_default_profile。
    void (async () => {
      const refreshList = async () => {
        try {
          const list = await invoke<ProfileEntry[]>('list_profiles');
          setProfileList(orderProfiles(list));
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
      setProfileList(orderProfiles(list));
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
        schema_version: 4,
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
    speakGlobalChange(globalEnabled);
  }, [globalEnabled]); // eslint-disable-line react-hooks/exhaustive-deps

  // Toggle 规则启动/停止时播报语音，通过 activeRuleIds 变化检测状态翻转。
  // 依赖 rules state 而非 ref，避免 queueMicrotask(initialLoadDone) 比 React re-render
  // 先触发时 rulesRef 为空导致 find 失败、speakToggle 永远不调用的竞态。
  useEffect(() => {
    if (!initialLoadDone.current) return;
    if (!globalEnabled) {
      prevActiveRuleIdsRef.current = new Set();
      return;
    }
    const prev = prevActiveRuleIdsRef.current;
    const curr = activeRuleIds;
    prevActiveRuleIdsRef.current = curr;

    // 先收集本次新启动的 toggle 规则
    const startedRules: BurstRule[] = [];
    for (const id of curr) {
      if (!prev.has(id)) {
        const rule = rules.find((r) => r.id === id);
        if (rule?.mode === 'toggle') startedRules.push(rule);
      }
    }

    // 停止播报：被同组新规则驱逐时跳过，避免覆盖后续的「开始」提示
    for (const id of prev) {
      if (!curr.has(id)) {
        const rule = rules.find((r) => r.id === id);
        if (rule?.mode !== 'toggle') continue;
        const displaced = rule.group != null && startedRules.some((r) => r.group === rule.group);
        if (!displaced) speakToggle(rule, false);
      }
    }

    // 启动播报放最后，保证是 speakLatest 最终出声的那条
    for (const rule of startedRules) {
      speakToggle(rule, true);
    }
  }, [activeRuleIds, globalEnabled, rules]); // eslint-disable-line react-hooks/exhaustive-deps

  function buildUtterance(text: string, s: SoundSettings): SpeechSynthesisUtterance {
    const utt = new SpeechSynthesisUtterance(text);
    utt.lang = 'zh-CN';
    utt.volume = s.volume / 100;
    // rate: -10..+10 → 0.5..1.5（中点 0 对应默认 1）
    utt.rate = 1 + s.rate * 0.05;
    // pitch: -10..+10 → 0..2（Web Speech API 原生属性，中点 0 对应默认 1）
    utt.pitch = 1 + s.pitch * 0.1;
    const voice = window.speechSynthesis.getVoices().find((v) => v.name === s.voiceName);
    if (voice) utt.voice = voice;
    return utt;
  }

  function speakLatest(text: string, s: SoundSettings) {
    if (!('speechSynthesis' in window)) return;
    const synth = window.speechSynthesis;
    synth.cancel();
    if (text.trim().length === 0) return;
    synth.speak(buildUtterance(text, s));
    if (synth.paused) synth.resume();
  }

  function speakGlobalChange(enabled: boolean) {
    const s = soundRef.current;
    if (!s.enabled) return;
    if (enabled ? !s.startEnabled : !s.endEnabled) return;
    speakLatest(enabled ? s.startText : s.endText, s);
  }

  function speakToggle(rule: BurstRule, isStart: boolean) {
    const s = soundRef.current;
    if (!s.enabled) return;
    if (isStart ? !s.toggleStartEnabled : !s.toggleEndEnabled) return;
    const template = (isStart ? s.toggleStartText : s.toggleEndText) ?? '';
    const text = template.split('${key}').join(keyLabel(rule.target_key));
    speakLatest(text, s);
  }

  function previewSound(type: 'start' | 'end' | 'toggleStart' | 'toggleEnd') {
    const s = soundRef.current;
    let text: string;
    if (type === 'start') text = s.startText;
    else if (type === 'end') text = s.endText;
    else if (type === 'toggleStart') text = s.toggleStartText.replace('${key}', 'F');
    else text = s.toggleEndText.replace('${key}', 'F');
    speakLatest(text, s);
  }

  function persistSound(patch: Partial<SoundSettings>) {
    // 副作用（写盘）放在 updater 外，避免 StrictMode 双调用导致重复写入。
    const next = { ...soundRef.current, ...patch };
    soundRef.current = next;
    setSound(next);
    settingsStore
      .set(SOUND_KEY, next)
      .then(() => settingsStore.save())
      .catch(() => {});
  }

  // 主题变更：立即应用（主色 / 亮暗）、持久化，并广播给其他窗口（浮窗）同步。
  function persistTheme(patch: Partial<ThemeSettings>) {
    const next = { ...theme, ...patch };
    setTheme(next);
    if (patch.color !== undefined) applyThemeColor(next.color);
    if (patch.mode !== undefined) {
      themeModeRef.current = next.mode;
      applyThemeMode(next.mode);
    }
    settingsStore
      .set(THEME_KEY, next)
      .then(() => settingsStore.save())
      .catch(() => {});
    emit('theme-changed', next).catch(() => {});
  }

  // 从下拉菜单设置明暗模式 / 主题色：应用+持久化并收起菜单。
  function pickThemeMode(mode: ThemeMode) {
    setMenuOpen(false);
    persistTheme({ mode });
  }

  function pickThemeColor(color: string) {
    setMenuOpen(false);
    persistTheme({ color });
  }

  async function handleToggleAutostart() {
    if (togglingAutostart) return;
    setTogglingAutostart(true);
    try {
      const next = await invoke<boolean>('toggle_autostart');
      setSysInfo((prev) => ({ ...prev, autostart_enabled: next }));
    } catch {
      toast.error('切换开机自启失败');
    } finally {
      setTogglingAutostart(false);
    }
  }

  function persistCloseBehavior(v: CloseBehavior | null) {
    const previous = closeBehaviorPreference;
    setCloseBehaviorPreference(v);
    const write =
      v === null
        ? settingsStore.delete(CLOSE_BEHAVIOR_KEY)
        : settingsStore.set(CLOSE_BEHAVIOR_KEY, v);
    write
      .then(() => settingsStore.save())
      .catch(() => {
        setCloseBehaviorPreference(previous);
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

  async function handleInstallDriver() {
    if (interceptionInstalled === 'pending_reboot') {
      await confirm({
        title: '请重启电脑',
        description: (
          <>
            检测到游戏模式驱动处于「待重启」状态——上次安装或卸载尚未完成，
            必须重启电脑后才能使用此驱动。
          </>
        ),
        confirmText: '知道了',
        cancelText: null,
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
    try {
      await invoke('install_driver');
    } catch (e) {
      toast.error(`安装失败：${e}`);
      return;
    }
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
      confirmText: '知道了',
      cancelText: null,
    });
  }

  async function selectInputMode(target: InputMode) {
    if (target === 'dd_hid') {
      setModePickerOpen(false);
      await showDdHidBlockedNotice();
      return;
    }
    // DD 系列与横版键鼠图互斥：横版下禁止切到 DD 驱动。
    if (isDdInputMode(target) && layout === 'horizontal') {
      setModePickerOpen(false);
      toast.warning('横版键鼠图不支持 DD 驱动模式，请先切回竖版规则列表');
      return;
    }
    if (switchingMode || target === inputMode) return;
    setSwitchingMode(true);
    try {
      // Interception 驱动：未安装则先安装并退出（要求重启电脑）
      if (target === 'interception' && interceptionInstalled !== 'installed') {
        await handleInstallDriver();
        return;
      }

      if ((target === 'interception' || target === 'ddsimple') && !elevated) {
        setModePickerOpen(false);
        const label = INPUT_MODE_LABELS[target];
        const ok = await confirm({
          title: `以管理员模式启用${label}`,
          description: (
            <>
              {label}
              需要管理员权限才能初始化驱动通道。应用将以管理员权限重启，并自动切换到{label}。
            </>
          ),
          confirmText: '以管理员重启',
          cancelText: '取消',
        });
        if (!ok) return;
        await invoke('relaunch_as_admin', { mode: target });
        return;
      }

      // 常规切换
      await invoke('set_input_mode', { mode: target });
      const status = await invoke<AppStatus>('get_app_status');
      applyAppStatus(status);
      const actual = status.input_mode;
      if (actual === target) {
        toast.success(`已切换为${INPUT_MODE_LABELS[target]}`);
      } else if (isInputMode(actual)) {
        toast.warning(
          target === 'interception'
            ? '驱动未就绪，请重启电脑后再试'
            : `切换未生效，已停留在${INPUT_MODE_LABELS[actual]}`,
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

  // ── 横版布局 ────────────────────────────────────────────────────────────────
  async function applyWindowSize(l: PanelLayout) {
    const s = l === 'horizontal' ? HORIZONTAL_SIZE : VERTICAL_SIZE;
    try {
      const w = getCurrentWindow();
      await w.setSize(new LogicalSize(s.width, s.height));
      await w.center();
    } catch {
      /* 尺寸调整失败不阻断交互 */
    }
  }

  function switchLayout(next: PanelLayout) {
    // DD 系列与横版键鼠图互斥：DD 驱动下禁止切到横版。
    if (next === 'horizontal' && isDdInputMode(inputMode)) {
      toast.warning('DD 驱动模式不支持横版键鼠图，请先切换到游戏模式或通用模式');
      return;
    }
    setLayout(next);
    void applyWindowSize(next);
    settingsStore
      .set(LAYOUT_KEY, next)
      .then(() => settingsStore.save())
      .catch(() => toast.warning('保存布局失败'));
  }

  // 单键规则（横版可表达的形态）+ 统一间隔显示值
  const hSingleRules = rules.filter((r) => keyEq(r.trigger_key, r.target_key));
  const hUnifiedInterval = hSingleRules[0]?.interval_ms ?? unifiedInterval;

  // 左键点键：无 → 切换 → 按压 → 无 轮换（高级规则键不响应）
  function handleHCycleKey(key: KeyId) {
    const onKey = rules.filter((r) => keyEq(r.trigger_key, key));
    const single = onKey.find((r) => keyEq(r.target_key, key));
    if (onKey.length > 1 || (onKey.length === 1 && !single)) return;
    if (!single) {
      pushRules((prev) => [...prev, newSingleKeyRule(key, 'toggle', hUnifiedInterval)]);
    } else if (single.mode === 'toggle') {
      void handleHSetMode(single, 'hold');
    } else {
      removeRule(single.id);
    }
  }

  // 切换连发 ↔ 按压连发；互斥组（仅切换连发可用）内改为按压时确认并移出分组
  async function handleHSetMode(rule: BurstRule, mode: BurstMode) {
    if (mode === 'hold' && rule.group) {
      const ok = await confirm({
        title: '移出互斥分组',
        description: `「${rule.group}」是互斥分组，仅切换连发可用。改为按压连发将把该键移出分组，是否继续？`,
        confirmText: '继续',
        tone: 'danger',
      });
      if (!ok) return;
      updateRule(rule.id, { mode, stop_key: null, enabled: true, group: null });
      return;
    }
    updateRule(rule.id, {
      mode,
      stop_key: mode === 'toggle' ? rule.trigger_key : null,
      enabled: true,
    });
  }

  function handleHSetEnabled(ruleId: string, enabled: boolean) {
    updateRule(ruleId, { enabled });
  }

  function handleHDeleteKey(key: KeyId) {
    pushRules((prev) => prev.filter((r) => !keyEq(r.trigger_key, key)));
  }

  // 统一间隔：批量写入所有单键规则
  function handleHSetInterval(ms: number) {
    setUnifiedInterval(ms);
    pushRules((prev) =>
      prev.map((r) => (keyEq(r.trigger_key, r.target_key) ? { ...r, interval_ms: ms } : r)),
    );
  }

  function handleHSetGroup(ruleId: string, group: string | null) {
    updateRule(ruleId, { group });
  }

  function handleHCreateGroupWith(ruleId: string) {
    const existingGroups = new Set(rules.map((r) => r.group).filter(Boolean));
    let n = 1;
    while (existingGroups.has(`互斥组${n}`)) n++;
    updateRule(ruleId, { group: `互斥组${n}` });
  }

  function handleNewGroup() {
    const existingGroups = new Set(rules.map((r) => r.group).filter(Boolean));
    let n = 1;
    while (existingGroups.has(`互斥组${n}`)) n++;
    const name = `互斥组${n}`;
    setPendingGroupName(name);
    setEditingGroupName({ current: name, draft: name });
  }

  async function disbandGroup(groupName: string) {
    const ok = await confirm({
      title: '解散分组',
      description: `将「${groupName}」解散，组内规则保留但不再互斥。`,
      confirmText: '解散',
      tone: 'danger',
    });
    if (!ok) return;
    pushRules((prev) => prev.map((r) => (r.group === groupName ? { ...r, group: null } : r)));
  }

  function toggleGroupCollapse(groupName: string) {
    setCollapsedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(groupName)) next.delete(groupName);
      else next.add(groupName);
      return next;
    });
  }

  function commitRenameGroup(oldName: string, newName: string) {
    const trimmed = newName.trim();
    const name = trimmed || oldName;
    if (pendingGroupName === oldName) setPendingGroupName(name);
    if (trimmed)
      pushRules((prev) => prev.map((r) => (r.group === oldName ? { ...r, group: name } : r)));
  }

  function handleDropBeforeRule(ruleId: string, targetRuleId: string) {
    pushRules((prev) => {
      const dragged = prev.find((r) => r.id === ruleId);
      const target = prev.find((r) => r.id === targetRuleId);
      if (!dragged || !target) return prev;
      const without = prev.filter((r) => r.id !== ruleId);
      const targetIdx = without.findIndex((r) => r.id === targetRuleId);
      const updated = { ...dragged, group: target.group };
      without.splice(targetIdx, 0, updated);
      return without;
    });
  }

  function handleDropToGroup(ruleId: string, groupName: string) {
    pushRules((prev) => {
      const dragged = prev.find((r) => r.id === ruleId);
      if (!dragged) return prev;
      const without = prev.filter((r) => r.id !== ruleId);
      const lastInGroup = without.reduce<number>(
        (acc, r, i) => (r.group === groupName ? i : acc),
        -1,
      );
      const updated = { ...dragged, group: groupName };
      if (lastInGroup === -1) return [...without, updated];
      without.splice(lastInGroup + 1, 0, updated);
      return without;
    });
  }

  function handleDropToUngrouped(ruleId: string) {
    pushRules((prev) => prev.map((r) => (r.id === ruleId ? { ...r, group: null } : r)));
  }

  function handleDropToPending(ruleId: string) {
    const name = editingGroupName?.draft.trim() || pendingGroupName;
    if (!name) return;
    setPendingGroupName(null);
    setEditingGroupName(null);
    pushRules((prev) => prev.map((r) => (r.id === ruleId ? { ...r, group: name } : r)));
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

  async function handleRenameProfileByName(name: string) {
    if (name === DEFAULT_PROFILE_NAME) {
      toast.warning('默认配置不可重命名');
      return;
    }
    let nextName = name;
    const ok = await confirm({
      title: '重命名配置',
      description: `配置「${name}」的新名称：`,
      confirmText: '重命名',
      body: (
        <ProfileNameForm
          defaultValue={name}
          onChange={(v) => {
            nextName = v;
          }}
        />
      ),
    });
    if (!ok) return;
    const trimmed = nextName.trim();
    if (!trimmed || trimmed === name) return;
    if (trimmed === DEFAULT_PROFILE_NAME) {
      toast.warning('不能使用默认配置名');
      return;
    }
    if (profileList.some((p) => p.meta.name === trimmed && p.meta.name !== name)) {
      toast.warning('已存在同名配置');
      return;
    }
    try {
      await invoke<string>('rename_profile', {
        oldName: name,
        newName: trimmed,
      });
      if (name === profileName) {
        setProfileName(trimmed);
        profileNameRef.current = trimmed;
      }
      await refreshProfileList();
      toast.success(`已重命名为「${trimmed}」`);
    } catch (e) {
      toast.error(`重命名失败：${e}`);
    }
  }

  async function handleDeleteProfileByName(name: string) {
    if (name === DEFAULT_PROFILE_NAME) {
      toast.warning('默认配置不可删除');
      return;
    }
    const ok = await confirm({
      title: '删除配置',
      description:
        name === profileName
          ? `确定删除当前配置「${name}」？删除后将自动切换为默认配置。`
          : `确定删除配置「${name}」？`,
      confirmText: '删除',
      tone: 'danger',
    });
    if (!ok) return;
    if (name === profileName && saveTimer.current) {
      clearTimeout(saveTimer.current);
      saveTimer.current = undefined;
    }
    if (name === profileName) initialLoadDone.current = false;
    try {
      const fallback = await invoke<Profile | null>('delete_profile', { name });
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
        else if (remembered === 'minimize') invoke('minimize_to_float').catch(() => {});
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
    else invoke('minimize_to_float').catch(() => {});
  }

  function handleShowSettings(tab: SettingsTab = 'general') {
    setMenuOpen(false);
    setProfileMenuOpen(false);
    setSettingsInitialTab(tab);
    setShowSettings(true);
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
        confirmText: '知道了',
        cancelText: null,
      });
    } catch (e) {
      toast.error(`卸载失败：${e}`);
    }
  }

  async function handleUninstallDdHid() {
    const ok = await confirm({
      title: '卸载DDHID 驱动',
      description: (
        <>
          将卸载DDHID 虚拟驱动。卸载后DDHID 模式将不可用，应用会切回通用模式。
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
        toast.success(r.message || 'DDHID 驱动已卸载');
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
      className={`panel ${layout === 'horizontal' ? 'panel--h' : 'panel--v'}${globalEnabled ? ' on' : ' off'}`}
      style={{ ['--panel-bg' as string]: `url(${bgUrl})` }}
    >
      <header className="panel-header" data-tauri-drag-region>
        <img className="header-icon" src={iconUrl} alt="" data-tauri-drag-region />
        <h1 data-tauri-drag-region>
          {APP_NAME}
          {hotkeys.panel_toggle && <Kbd label="显隐">{keyLabel(hotkeys.panel_toggle)}</Kbd>}
        </h1>
        <div className="window-controls">
          <button
            className="win-btn"
            onClick={() => switchLayout(layout === 'vertical' ? 'horizontal' : 'vertical')}
            disabled={isDdInputMode(inputMode) && layout === 'vertical'}
            aria-label="切换布局"
            title={
              isDdInputMode(inputMode) && layout === 'vertical'
                ? 'DD 驱动模式不支持横版键鼠图'
                : layout === 'vertical'
                  ? '切换到横版键鼠图'
                  : '切换到竖版规则列表'
            }
          >
            {layout === 'vertical' ? (
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                <rect x="1.5" y="3" width="13" height="4" rx="1" stroke="currentColor" />
                <rect x="1.5" y="9" width="13" height="4" rx="1" stroke="currentColor" />
              </svg>
            ) : (
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                <rect x="3" y="1.5" width="4" height="13" rx="1" stroke="currentColor" />
                <rect x="9" y="1.5" width="4" height="13" rx="1" stroke="currentColor" />
              </svg>
            )}
          </button>
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
            onClick={() => invoke('minimize_to_float').catch(() => {})}
            aria-label="最小化到浮窗"
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
        {layout === 'vertical' && (
          <Tabs
            tabs={(['hold', 'toggle'] as BurstMode[]).map((mode) => {
              const groupRules = rules.filter((r) => r.mode === mode);
              const active = groupRules.filter((r) => r.enabled).length;
              return {
                id: mode,
                label: mode === 'hold' ? '按压连发' : '切换连发',
                badge: `${active}/${groupRules.length}`,
              };
            })}
            active={activeTab}
            grow
            onChange={(mode) => {
              setActiveTab(mode);
              settingsStore
                .set(ACTIVE_TAB_KEY, mode)
                .then(() => settingsStore.save())
                .catch(() => toast.warning('保存当前标签页失败'));
            }}
          />
        )}

        {layout === 'horizontal' && (
          <HorizontalLayout
            rules={rules}
            activeRuleIds={activeRuleIds}
            conflicts={conflicts}
            interval={hUnifiedInterval}
            intervalMin={MIN_INTERVAL_MS}
            intervalMax={MAX_INTERVAL_MS}
            onIntervalChange={handleHSetInterval}
            onCycleKey={handleHCycleKey}
            onSetEnabled={handleHSetEnabled}
            onSetMode={handleHSetMode}
            onSetGroup={handleHSetGroup}
            onCreateGroupWith={handleHCreateGroupWith}
            onDeleteRule={removeRule}
            onDeleteKey={handleHDeleteKey}
          />
        )}

        {layout === 'vertical' &&
          (['hold', 'toggle'] as BurstMode[]).map((mode) => {
            if (mode !== activeTab) return null;
            if (mode === 'hold') {
              const holdRules = rules.filter((r) => r.mode === 'hold');
              return (
                <div className="rule-group" key="hold">
                  <div className="rules-list">
                    {holdRules.length === 0 && <p className="empty">暂无按压连发规则</p>}
                    {holdRules.map((rule) => {
                      const isActive = activeRuleIds.has(rule.id);
                      const showAdvanced = advancedOpen[rule.id];
                      const isDragging = draggingId === rule.id;
                      const isDragTarget =
                        dragOverInfo?.kind === 'rule' &&
                        dragOverInfo.ruleId === rule.id &&
                        draggingId !== rule.id;
                      return (
                        <div
                          key={rule.id}
                          className={`rule-row${rule.enabled ? '' : ' disabled'}${isActive ? ' active' : ''}${isDragging ? ' dragging' : ''}${isDragTarget ? ' drag-target' : ''}`}
                          draggable
                          onDragStart={(e) => {
                            draggingIdRef.current = rule.id;
                            e.dataTransfer.setData('text/plain', rule.id);
                            e.dataTransfer.effectAllowed = 'move';
                            setDraggingId(rule.id);
                          }}
                          onDragOver={(e) => {
                            e.preventDefault();
                            setDragOverInfo({ kind: 'rule', ruleId: rule.id });
                          }}
                          onDrop={(e) => {
                            e.preventDefault();
                            e.stopPropagation();
                            const srcId =
                              draggingIdRef.current || e.dataTransfer.getData('text/plain');
                            draggingIdRef.current = null;
                            if (srcId && srcId !== rule.id) handleDropBeforeRule(srcId, rule.id);
                            setDraggingId(null);
                            setDragOverInfo(null);
                          }}
                          onDragEnd={() => {
                            draggingIdRef.current = null;
                            setDraggingId(null);
                            setDragOverInfo(null);
                          }}
                        >
                          <span className="drag-handle" aria-hidden>
                            ⠿
                          </span>
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
                              <div className="rule-field">
                                <label>连发按键</label>
                                <KeyCapture
                                  value={rule.target_key}
                                  onChange={(vk) => {
                                    if (!vk) return;
                                    const patch: Partial<BurstRule> = { target_key: vk };
                                    if (!showAdvanced) patch.trigger_key = vk;
                                    updateRule(rule.id, patch);
                                  }}
                                  conflict={
                                    !showAdvanced ? severityForRule(conflicts, rule.id) : null
                                  }
                                />
                              </div>
                              <div className="rule-field rule-interval">
                                <label>间隔</label>
                                <div className="interval-input">
                                  <input
                                    type="number"
                                    min={MIN_INTERVAL_MS}
                                    max={MAX_INTERVAL_MS}
                                    value={rule.interval_ms}
                                    onChange={(e) =>
                                      updateRule(rule.id, {
                                        interval_ms: Math.max(
                                          MIN_INTERVAL_MS,
                                          Math.min(MAX_INTERVAL_MS, Number(e.target.value)),
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
                          {showAdvanced && (
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
                    onClick={() => addRule('hold')}
                  >
                    + 添加按压连发规则
                  </Button>
                </div>
              );
            }

            // Toggle tab — 分组容器 UI
            const toggleRules = rules.filter((r) => r.mode === 'toggle');
            const ungroupedRules = toggleRules.filter((r) => !r.group);
            const groupNames = [
              ...new Set(toggleRules.filter((r) => r.group).map((r) => r.group as string)),
            ];
            const draggingRule = draggingId ? rules.find((r) => r.id === draggingId) : null;

            const renderToggleCard = (rule: BurstRule) => {
              const isActive = activeRuleIds.has(rule.id);
              const showAdvanced = advancedOpen[rule.id];
              const isDragging = draggingId === rule.id;
              const isDragTarget =
                dragOverInfo?.kind === 'rule' &&
                dragOverInfo.ruleId === rule.id &&
                draggingId !== rule.id;
              return (
                <div
                  key={rule.id}
                  className={`rule-row${rule.enabled ? '' : ' disabled'}${isActive ? ' active' : ''}${isDragging ? ' dragging' : ''}${isDragTarget ? ' drag-target' : ''}`}
                  draggable
                  onDragStart={(e) => {
                    draggingIdRef.current = rule.id;
                    e.dataTransfer.setData('text/plain', rule.id);
                    e.dataTransfer.effectAllowed = 'move';
                    setDraggingId(rule.id);
                  }}
                  onDragOver={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    e.dataTransfer.dropEffect = 'move';
                    setDragOverInfo({ kind: 'rule', ruleId: rule.id });
                  }}
                  onDrop={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    const srcId = draggingIdRef.current || e.dataTransfer.getData('text/plain');
                    draggingIdRef.current = null;
                    if (srcId && srcId !== rule.id) handleDropBeforeRule(srcId, rule.id);
                    setDraggingId(null);
                    setDragOverInfo(null);
                  }}
                  onDragEnd={() => {
                    draggingIdRef.current = null;
                    setDraggingId(null);
                    setDragOverInfo(null);
                  }}
                >
                  <span className="drag-handle" aria-hidden>
                    ⠿
                  </span>
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
                      <div className="rule-field rule-interval">
                        <label>间隔</label>
                        <div className="interval-input">
                          <input
                            type="number"
                            min={MIN_INTERVAL_MS}
                            max={MAX_INTERVAL_MS}
                            value={rule.interval_ms}
                            onChange={(e) =>
                              updateRule(rule.id, {
                                interval_ms: Math.max(
                                  MIN_INTERVAL_MS,
                                  Math.min(MAX_INTERVAL_MS, Number(e.target.value)),
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
                  {showAdvanced && (
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
            };

            return (
              <div className="rule-group" key="toggle">
                <div className="rules-list">
                  {toggleRules.length === 0 && !pendingGroupName && (
                    <p className="empty">暂无切换连发规则</p>
                  )}

                  {/* 无分组规则 */}
                  {ungroupedRules.map(renderToggleCard)}

                  {/* 拖动有分组规则时显示"移出分组"区域 */}
                  {draggingId && draggingRule?.group && (
                    <div
                      className={`ungrouped-escape-zone${dragOverInfo?.kind === 'ungrouped' ? ' drag-active' : ''}`}
                      onDragOver={(e) => {
                        e.preventDefault();
                        setDragOverInfo({ kind: 'ungrouped' });
                      }}
                      onDrop={(e) => {
                        e.preventDefault();
                        const srcId = draggingIdRef.current || e.dataTransfer.getData('text/plain');
                        draggingIdRef.current = null;
                        if (srcId) handleDropToUngrouped(srcId);
                        setDraggingId(null);
                        setDragOverInfo(null);
                      }}
                    >
                      移出分组
                    </div>
                  )}

                  {/* 分组容器 */}
                  {groupNames.map((groupName) => {
                    const groupRules = toggleRules.filter((r) => r.group === groupName);
                    const isEditing = editingGroupName?.current === groupName;
                    const isGroupDragOver =
                      dragOverInfo?.kind === 'group' && dragOverInfo.name === groupName;
                    const isCollapsed = collapsedGroups.has(groupName);
                    return (
                      <div
                        key={groupName}
                        className={`rule-group-container${isGroupDragOver ? ' drag-active' : ''}`}
                        onDragOver={(e) => {
                          e.preventDefault();
                          setDragOverInfo({ kind: 'group', name: groupName });
                        }}
                        onDrop={(e) => {
                          e.preventDefault();
                          const srcId =
                            draggingIdRef.current || e.dataTransfer.getData('text/plain');
                          draggingIdRef.current = null;
                          if (srcId) handleDropToGroup(srcId, groupName);
                          setDraggingId(null);
                          setDragOverInfo(null);
                        }}
                      >
                        <div
                          className={`rule-group-header${isCollapsed ? ' collapsed' : ''}`}
                          onClick={() => !isEditing && toggleGroupCollapse(groupName)}
                        >
                          {isEditing ? (
                            <input
                              className="group-name-edit"
                              autoFocus
                              value={editingGroupName.draft}
                              onChange={(e) =>
                                setEditingGroupName((g) => g && { ...g, draft: e.target.value })
                              }
                              onBlur={() => {
                                if (editingGroupName) {
                                  commitRenameGroup(
                                    editingGroupName.current,
                                    editingGroupName.draft,
                                  );
                                  setEditingGroupName(null);
                                }
                              }}
                              onKeyDown={(e) => {
                                if (e.key === 'Enter' && editingGroupName) {
                                  commitRenameGroup(
                                    editingGroupName.current,
                                    editingGroupName.draft,
                                  );
                                  setEditingGroupName(null);
                                } else if (e.key === 'Escape') {
                                  setEditingGroupName(null);
                                }
                              }}
                              onClick={(e) => e.stopPropagation()}
                              onDragOver={(e) => e.preventDefault()}
                              onDrop={(e) => e.preventDefault()}
                            />
                          ) : (
                            <div
                              className={`group-collapse-indicator${isCollapsed ? ' collapsed' : ''}`}
                            >
                              <ChevronIcon size={12} className="group-chevron" />
                              <span className="group-name-text">{groupName}</span>
                            </div>
                          )}
                          {!isEditing && (
                            <button
                              className="group-edit-btn"
                              onClick={(e) => {
                                e.stopPropagation();
                                setEditingGroupName({ current: groupName, draft: groupName });
                              }}
                              title="重命名"
                            >
                              <EditIcon size={12} />
                            </button>
                          )}
                          <button
                            className="disband-btn"
                            onClick={(e) => {
                              e.stopPropagation();
                              disbandGroup(groupName);
                            }}
                            title="解散分组（规则保留）"
                          >
                            解散
                          </button>
                        </div>
                        {!isCollapsed && (
                          <div className="group-body">
                            {groupRules.map(renderToggleCard)}
                            <div
                              className={`group-drop-zone${isGroupDragOver ? ' drag-active' : ''}`}
                              onDragOver={(e) => {
                                e.preventDefault();
                                e.stopPropagation();
                                setDragOverInfo({ kind: 'group', name: groupName });
                              }}
                              onDrop={(e) => {
                                e.preventDefault();
                                e.stopPropagation();
                                const srcId =
                                  draggingIdRef.current || e.dataTransfer.getData('text/plain');
                                draggingIdRef.current = null;
                                if (srcId) handleDropToGroup(srcId, groupName);
                                setDraggingId(null);
                                setDragOverInfo(null);
                              }}
                            >
                              拖入规则
                            </div>
                          </div>
                        )}
                      </div>
                    );
                  })}

                  {/* 待命空分组（新建后等待拖入或改名） */}
                  {pendingGroupName &&
                    (() => {
                      const isPendingEditing = editingGroupName?.current === pendingGroupName;
                      const isPendingDragOver =
                        dragOverInfo?.kind === 'group' && dragOverInfo.name === pendingGroupName;
                      return (
                        <div
                          key="__pending__"
                          className={`rule-group-container${isPendingDragOver ? ' drag-active' : ''}`}
                          onDragOver={(e) => {
                            e.preventDefault();
                            setDragOverInfo({ kind: 'group', name: pendingGroupName });
                          }}
                          onDrop={(e) => {
                            e.preventDefault();
                            const srcId =
                              draggingIdRef.current || e.dataTransfer.getData('text/plain');
                            draggingIdRef.current = null;
                            if (srcId) handleDropToPending(srcId);
                            setDraggingId(null);
                            setDragOverInfo(null);
                          }}
                        >
                          <div className="rule-group-header">
                            {isPendingEditing ? (
                              <input
                                className="group-name-edit"
                                autoFocus
                                value={editingGroupName.draft}
                                onChange={(e) =>
                                  setEditingGroupName({
                                    current: pendingGroupName,
                                    draft: e.target.value,
                                  })
                                }
                                onBlur={() => {
                                  const name =
                                    (editingGroupName?.draft ?? '').trim() || pendingGroupName;
                                  setPendingGroupName(name);
                                  setEditingGroupName(null);
                                }}
                                onKeyDown={(e) => {
                                  if (e.key === 'Enter') {
                                    const name =
                                      (editingGroupName?.draft ?? '').trim() || pendingGroupName;
                                    setPendingGroupName(name);
                                    setEditingGroupName(null);
                                  } else if (e.key === 'Escape') {
                                    setPendingGroupName(null);
                                    setEditingGroupName(null);
                                  }
                                }}
                                onDragOver={(e) => e.preventDefault()}
                                onDrop={(e) => e.preventDefault()}
                              />
                            ) : (
                              <span className="group-name-text">{pendingGroupName}</span>
                            )}
                            {!isPendingEditing && (
                              <button
                                className="group-edit-btn"
                                onClick={() =>
                                  setEditingGroupName({
                                    current: pendingGroupName,
                                    draft: pendingGroupName,
                                  })
                                }
                                title="重命名"
                              >
                                <EditIcon size={12} />
                              </button>
                            )}
                            <button
                              className="disband-btn"
                              onClick={() => {
                                setPendingGroupName(null);
                                setEditingGroupName(null);
                              }}
                              title="取消"
                            >
                              取消
                            </button>
                          </div>
                          <div className="group-body">
                            <div
                              className={`group-drop-zone${isPendingDragOver ? ' drag-active' : ''}`}
                              onDragOver={(e) => {
                                e.preventDefault();
                                e.stopPropagation();
                                setDragOverInfo({ kind: 'group', name: pendingGroupName });
                              }}
                              onDrop={(e) => {
                                e.preventDefault();
                                e.stopPropagation();
                                const srcId =
                                  draggingIdRef.current || e.dataTransfer.getData('text/plain');
                                draggingIdRef.current = null;
                                if (srcId) handleDropToPending(srcId);
                                setDraggingId(null);
                                setDragOverInfo(null);
                              }}
                            >
                              将规则拖入此分组
                            </div>
                          </div>
                        </div>
                      );
                    })()}
                </div>

                <div className="rules-bottom-actions">
                  <Button
                    className="add-btn"
                    variant="dashed"
                    tone="primary"
                    onClick={() => addRule('toggle')}
                  >
                    + 添加切换连发规则
                  </Button>
                  <Button
                    className="add-btn"
                    variant="dashed"
                    tone="neutral"
                    onClick={handleNewGroup}
                  >
                    + 新建互斥分组
                  </Button>
                </div>
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
              {elevated && (inputMode === 'ddsimple' || inputMode === 'dd_hid') ? ' ★' : ''}
            </Button>
          </div>
          <div className="footer-control">
            <Button
              variant="solid"
              tone={globalEnabled ? 'primary' : 'neutral'}
              size="sm"
              loading={togglingGlobal}
              kbd={(() => {
                const k = globalEnabled
                  ? (hotkeys.global_stop ?? hotkeys.global_toggle)
                  : hotkeys.global_toggle;
                return k ? keyLabel(k) : undefined;
              })()}
              onClick={toggleGlobal}
            >
              {globalEnabled ? '全局已启用' : '全局已禁用'}
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
          onSwitch: switchToProfile,
          onCreate: handleCreateProfile,
          onManage: () => handleShowSettings('profiles'),
        })}
      />

      <ContextMenu
        open={modePickerOpen}
        onClose={() => setModePickerOpen(false)}
        target={modeBtnRef}
        location="bottom-left"
        items={(['interception', 'sendinput', 'ddsimple'] as InputMode[]).map((m) => ({
          label: INPUT_MODE_LABELS[m],
          subtitle:
            m === 'sendinput'
              ? '最简单，但很多游戏不响应'
              : m === 'interception'
                ? interceptionInstalled === 'installed'
                  ? elevated
                    ? '推荐 · 管理员已就绪'
                    : '推荐 · 需要管理员'
                  : interceptionInstalled === 'pending_reboot'
                    ? '推荐 · 驱动待重启生效'
                    : '推荐 · 点击安装驱动'
                : elevated
                  ? '备用 · 内置DD驱动，无需重启'
                  : '备用 · 需要管理员',
          active: inputMode === m,
          onClick: () => void selectInputMode(m),
        }))}
      />

      <ContextMenu
        open={menuOpen}
        onClose={() => setMenuOpen(false)}
        target={menuBtnRef}
        items={[
          { label: '设置', onClick: handleShowSettings },
          {
            label: '主题颜色',
            children: [
              {
                label: '明亮',
                active: theme.mode === 'light',
                onClick: () => pickThemeMode('light'),
              },
              {
                label: '黑暗',
                active: theme.mode === 'dark',
                onClick: () => pickThemeMode('dark'),
              },
              {
                label: '跟随系统',
                active: theme.mode === 'system',
                onClick: () => pickThemeMode('system'),
              },
              { type: 'divider' },
              ...SECT_PRESETS.map((p) => ({
                label: p.name,
                active: theme.color.toLowerCase() === p.color.toLowerCase(),
                prependIcon: (
                  <span
                    aria-hidden="true"
                    style={{
                      width: 12,
                      height: 12,
                      borderRadius: 3,
                      background: p.color,
                      boxShadow: 'inset 0 0 0 1px rgba(0, 0, 0, 0.15)',
                    }}
                  />
                ),
                onClick: () => pickThemeColor(p.color),
              })),
            ],
          },
          { type: 'divider' },
          { label: '检查更新', onClick: handleCheckUpdate },
          {
            label: '更新公告',
            appendIcon: updateNotice ? (
              <span
                aria-hidden="true"
                style={{ width: 6, height: 6, borderRadius: '50%', background: '#6c4de6' }}
              />
            ) : undefined,
            onClick: () => {
              setMenuOpen(false);
              if (updateNotice) setShowUpdateNotice(true);
            },
          },
          {
            label: '用户协议',
            onClick: () => {
              setMenuOpen(false);
              setShowAgreement(true);
            },
          },
          { type: 'divider' },
          { label: '诊断修复', onClick: handleShowRepair },
          { label: '关于', onClick: handleShowAbout },
        ]}
      />

      <Overlay open={showSettings} onClose={() => setShowSettings(false)}>
        <SettingsDialog
          initialTab={settingsInitialTab}
          appVersion={appVersion}
          inputMode={inputMode}
          layout={layout}
          switchingMode={switchingMode}
          globalEnabled={globalEnabled}
          togglingGlobal={togglingGlobal}
          elevated={elevated}
          closeBehavior={closeBehaviorPreference}
          interceptionInstalled={interceptionInstalled}
          ddHidInstalled={ddHidInstalled}
          autostartEnabled={sysInfo.autostart_enabled}
          togglingAutostart={togglingAutostart}
          sound={sound}
          availableVoices={availableVoices}
          profiles={profileList}
          profileName={profileName}
          profileCount={profileList.length}
          isDefaultProfile={isDefaultProfile}
          onClose={() => setShowSettings(false)}
          onSelectInputMode={(mode) => {
            setShowSettings(false);
            void selectInputMode(mode);
          }}
          hotkeys={hotkeys}
          hotkeyConflicts={{
            global_toggle: severityForKey(conflicts, hotkeys.global_toggle),
            global_stop: severityForKey(conflicts, hotkeys.global_stop),
            panel_toggle: severityForKey(conflicts, hotkeys.panel_toggle),
          }}
          onHotkeyChange={(patch) => {
            void ensureWritableProfile();
            setHotkeys((prev) => ({ ...prev, ...patch }));
          }}
          onToggleGlobal={() => void toggleGlobal()}
          onSetCloseBehavior={persistCloseBehavior}
          onToggleAutostart={() => void handleToggleAutostart()}
          onSoundChange={persistSound}
          onPreviewSound={previewSound}
          theme={theme}
          onThemeChange={persistTheme}
          onCreateProfile={() => {
            setShowSettings(false);
            void handleCreateProfile();
          }}
          onImportProfile={() => {
            setShowSettings(false);
            setShowImport(true);
          }}
          onSwitchProfile={(path) => void switchToProfile(path)}
          onRenameProfile={(name) => void handleRenameProfileByName(name)}
          onDeleteProfile={(name) => void handleDeleteProfileByName(name)}
        />
      </Overlay>

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
              platform: sysInfo.platform,
              os_family: sysInfo.os_family,
              os_version: sysInfo.os_version,
              webview_version: sysInfo.webview_version,
              arch: sysInfo.arch,
              locale: sysInfo.locale,
              install_path: sysInfo.install_path,
              log_dir: sysInfo.log_dir,
              app_data_dir: sysInfo.app_data_dir,
              resources_ok: sysInfo.resources_ok,
              missing_resources: sysInfo.missing_resources,
              scheduler_hp_degraded: sysInfo.scheduler_hp_degraded,
            } satisfies AboutDialogInfo
          }
          updateNotice={updateNotice}
          checkingUpdate={updateProgress !== null && !updateProgress.done}
          onClose={() => setShowAbout(false)}
          onCheckUpdate={() => {
            setShowAbout(false);
            handleCheckUpdate();
          }}
          onShowUpdateNotice={() => {
            setShowAbout(false);
            if (updateNotice) setShowUpdateNotice(true);
          }}
          onShowAgreement={() => {
            setShowAbout(false);
            setShowAgreement(true);
          }}
          onOpenDir={(kind) => {
            invoke('open_app_dir', { kind }).catch((err) => {
              toast.warning(`打开目录失败：${err}`);
            });
          }}
          onCopied={() => toast.success('已复制环境信息')}
          onCopyFailed={(e) => toast.error(`复制失败：${e}`)}
        />
      </Overlay>

      <Overlay open={showRepair} onClose={() => setShowRepair(false)}>
        <RepairDialog
          elevated={elevated}
          autostartEnabled={sysInfo.autostart_enabled}
          inputMode={inputMode}
          interceptionInstalled={interceptionInstalled}
          ddHidInstalled={ddHidInstalled}
          onClose={() => setShowRepair(false)}
          onToast={(kind, msg) => {
            if (kind === 'success') toast.success(msg);
            else if (kind === 'warn') toast.warning(msg);
            else toast.error(msg);
          }}
          onInstallDriver={() => void handleInstallDriver()}
          onUninstallDriver={() => void handleUninstallDriver()}
          onUninstallDdHid={() => void handleUninstallDdHid()}
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
