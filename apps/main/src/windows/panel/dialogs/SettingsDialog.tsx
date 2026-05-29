import { type ReactNode, useState } from 'react';
import Button from '../components/Button';
import { CardList, CardListButton } from '../components/CardList';
import type { CloseBehavior } from '../components/CloseBehaviorForm';
import KeyCapture, { type KeyId } from '../components/KeyCapture';
import Tabs from '../components/Tabs';
import type { ConflictSeverity } from '../conflicts';
import DialogShell from './DialogShell';
import ProfileCardList, { type SettingsProfileEntry } from './ProfileCardList';
import './SettingsDialog.css';

export type SettingsTab = 'general' | 'hotkeys' | 'sound' | 'profiles';
type SettingsInputMode = 'sendinput' | 'interception' | 'dd_hid';
type DriverStatus = 'installed' | 'pending_reboot' | 'not_installed';

export interface SoundSettings {
  enabled: boolean;
  volume: number;
  rate: number;
  pitch: number;
  startText: string;
  endText: string;
  voiceName: string;
  globalOnly: boolean; // reserved, not yet wired
}

interface Props {
  initialTab?: SettingsTab;
  appVersion: string;
  inputMode: SettingsInputMode;
  switchingMode: boolean;
  globalEnabled: boolean;
  togglingGlobal: boolean;
  closeBehavior: CloseBehavior | null;
  elevated: boolean;
  interceptionInstalled: DriverStatus;
  ddHidInstalled: DriverStatus;
  autostartEnabled: boolean;
  togglingAutostart: boolean;
  sound: SoundSettings;
  availableVoices: string[];
  profiles: SettingsProfileEntry[];
  profileName: string;
  profileCount: number;
  isDefaultProfile: boolean;
  onClose: () => void;
  onSelectInputMode: (mode: SettingsInputMode) => void;
  onToggleGlobal: () => void;
  onSetCloseBehavior: (choice: CloseBehavior | null) => void;
  hotkeys: { global_toggle: KeyId | null; global_stop: KeyId | null; panel_toggle: KeyId | null };
  hotkeyConflicts: {
    global_toggle: ConflictSeverity | null;
    global_stop: ConflictSeverity | null;
    panel_toggle: ConflictSeverity | null;
  };
  onHotkeyChange: (patch: {
    global_toggle?: KeyId | null;
    global_stop?: KeyId | null;
    panel_toggle?: KeyId | null;
  }) => void;
  onToggleAutostart: () => void;
  onSoundChange: (patch: Partial<SoundSettings>) => void;
  onPreviewSound: (type: 'start' | 'end') => void;
  onCreateProfile: () => void;
  onImportProfile: () => void;
  onSwitchProfile: (path: string) => void;
  onRenameProfile: (name: string) => void;
  onDeleteProfile: (name: string) => void;
}

const TABS: { id: SettingsTab; label: string }[] = [
  { id: 'general', label: '通用' },
  { id: 'hotkeys', label: '热键' },
  { id: 'sound', label: '声音' },
  { id: 'profiles', label: '配置文件' },
];

const DEFAULT_PROFILE_NAME = 'defults';

const INPUT_MODE_LABELS: Record<SettingsInputMode, string> = {
  sendinput: '通用模式',
  interception: '游戏模式',
  dd_hid: '究极HID',
};

const INPUT_MODE_HINTS: Record<SettingsInputMode, string> = {
  sendinput: 'SendInput',
  interception: 'Interception',
  dd_hid: 'DD-HID',
};

const CLOSE_BEHAVIOR_OPTIONS: {
  value: CloseBehavior | null;
  label: string;
  detail: string;
}[] = [
  { value: 'minimize', label: '最小化到托盘', detail: '后台继续运行' },
  { value: 'exit', label: '直接退出', detail: '关闭应用进程' },
  { value: null, label: '关闭时询问', detail: '每次确认' },
];

function SettingsSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="settings-section">
      <h3 className="settings-section-title">{title}</h3>
      {children}
    </section>
  );
}

function driverStatusLabel(status: DriverStatus): string {
  if (status === 'installed') return '已安装';
  if (status === 'pending_reboot') return '待重启';
  return '未安装';
}

function modeDetail(mode: SettingsInputMode, props: Props): string {
  if (mode === 'interception') {
    return props.interceptionInstalled === 'installed'
      ? '驱动已就绪'
      : driverStatusLabel(props.interceptionInstalled);
  }
  if (mode === 'dd_hid') {
    if (props.ddHidInstalled !== 'installed') return driverStatusLabel(props.ddHidInstalled);
    return props.elevated ? '管理员已就绪' : '需要管理员';
  }
  return '无需驱动';
}

function SliderRow({
  label,
  value,
  min,
  max,
  onChange,
  formatValue,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  onChange: (v: number) => void;
  formatValue?: (v: number) => string;
}) {
  return (
    <div className="settings-row">
      <div className="settings-row-main">
        <span className="settings-row-title">{label}</span>
      </div>
      <div className="settings-slider-group">
        <input
          type="range"
          className="settings-slider"
          min={min}
          max={max}
          value={value}
          onChange={(e) => onChange(Number(e.target.value))}
        />
        <span className="settings-slider-value">{formatValue ? formatValue(value) : value}</span>
      </div>
    </div>
  );
}

export default function SettingsDialog(props: Props) {
  const [tab, setTab] = useState<SettingsTab>(props.initialTab ?? 'general');
  const { sound } = props;

  const tabsNode = (
    <Tabs tabs={TABS} active={tab} onChange={setTab} variant="pill" grow />
  );

  return (
    <DialogShell
      className="settings-card"
      title="设置"
      labelId="settings-title"
      subheader={tabsNode}
      footer={<Button onClick={props.onClose}>关闭</Button>}
    >
      <div className="settings-body">
        {tab === 'general' && (
          <>
            <SettingsSection title="运行">
              <div className="settings-row">
                <div className="settings-row-main">
                  <span className="settings-row-title">全局开关</span>
                  <span className="settings-row-desc">
                    {props.globalEnabled ? '当前启用' : '当前停用'}
                  </span>
                </div>
                <Button
                  size="sm"
                  tone={props.globalEnabled ? 'primary' : 'neutral'}
                  loading={props.togglingGlobal}
                  onClick={props.onToggleGlobal}
                >
                  {props.globalEnabled ? '已启用' : '已禁用'}
                </Button>
              </div>
              <div className="settings-row">
                <div className="settings-row-main">
                  <span className="settings-row-title">开机自启</span>
                  <span className="settings-row-desc">
                    {props.autostartEnabled ? '登录后自动启动' : '不自动启动'}
                  </span>
                </div>
                <Button
                  size="sm"
                  tone={props.autostartEnabled ? 'primary' : 'neutral'}
                  loading={props.togglingAutostart}
                  onClick={props.onToggleAutostart}
                >
                  {props.autostartEnabled ? '已启用' : '已禁用'}
                </Button>
              </div>
            </SettingsSection>

            <SettingsSection title="输入模式">
              <CardList>
                {(['sendinput', 'interception', 'dd_hid'] as SettingsInputMode[]).map((mode) => (
                  <CardListButton
                    key={mode}
                    active={props.inputMode === mode}
                    className="settings-mode"
                    disabled={props.switchingMode}
                    onClick={() => props.onSelectInputMode(mode)}
                  >
                    <span className="settings-mode-name">{INPUT_MODE_LABELS[mode]}</span>
                    <span className="settings-mode-meta">
                      {INPUT_MODE_HINTS[mode]} · {modeDetail(mode, props)}
                    </span>
                  </CardListButton>
                ))}
              </CardList>
              <p className="settings-note">驱动安装与卸载操作请前往「诊断修复」。</p>
            </SettingsSection>

            <SettingsSection title="关闭行为">
              <CardList columns="three" role="radiogroup" aria-label="关闭行为">
                {CLOSE_BEHAVIOR_OPTIONS.map((item) => (
                  <CardListButton
                    key={item.value ?? 'ask'}
                    role="radio"
                    aria-checked={props.closeBehavior === item.value}
                    active={props.closeBehavior === item.value}
                    className="settings-choice"
                    onClick={() => props.onSetCloseBehavior(item.value)}
                  >
                    <span>{item.label}</span>
                    <small>{item.detail}</small>
                  </CardListButton>
                ))}
              </CardList>
            </SettingsSection>

          </>
        )}

        {tab === 'hotkeys' && (
          <SettingsSection title="热键">
            <div className="settings-hotkey-list">
              <div className="settings-hotkey-row">
                <span className="settings-hotkey-label">全局开关</span>
                <div className="settings-hotkey-keys">
                  <KeyCapture
                    value={props.hotkeys.global_toggle}
                    nullable
                    placeholder="未设置"
                    conflict={props.hotkeyConflicts.global_toggle}
                    onChange={(k) =>
                      props.onHotkeyChange({
                        global_toggle: k,
                        global_stop: k === null ? null : props.hotkeys.global_stop,
                      })
                    }
                  />
                  {props.hotkeys.global_toggle && (
                    <>
                      <span className="settings-hotkey-sep">停止</span>
                      <KeyCapture
                        value={props.hotkeys.global_stop}
                        nullable
                        placeholder="同开启键"
                        conflict={props.hotkeyConflicts.global_stop}
                        onChange={(k) => props.onHotkeyChange({ global_stop: k })}
                      />
                    </>
                  )}
                </div>
              </div>
              <div className="settings-hotkey-row">
                <span className="settings-hotkey-label">面板显隐</span>
                <div className="settings-hotkey-keys">
                  <KeyCapture
                    value={props.hotkeys.panel_toggle}
                    nullable
                    placeholder="未设置"
                    conflict={props.hotkeyConflicts.panel_toggle}
                    onChange={(k) => props.onHotkeyChange({ panel_toggle: k })}
                  />
                </div>
              </div>
            </div>
            <p className="settings-note">热键随当前配置文件保存。</p>
          </SettingsSection>
        )}

        {tab === 'sound' && (
          <>
            <SettingsSection title="声音反馈">
              <div className="settings-row">
                <div className="settings-row-main">
                  <span className="settings-row-title">声音反馈</span>
                  <span className="settings-row-desc">
                    {sound.enabled ? '切换全局开关时朗读语句' : '已关闭'}
                  </span>
                </div>
                <Button
                  size="sm"
                  tone={sound.enabled ? 'primary' : 'neutral'}
                  onClick={() => props.onSoundChange({ enabled: !sound.enabled })}
                >
                  {sound.enabled ? '已启用' : '已禁用'}
                </Button>
              </div>
            </SettingsSection>

            <SettingsSection title="语句">
              <div className={`settings-sound-body${sound.enabled ? '' : ' settings-sound-body--disabled'}`}>
                <div className="settings-row">
                  <div className="settings-row-main">
                    <span className="settings-row-title">开始语句</span>
                    <span className="settings-row-desc">全局开关启用时朗读</span>
                  </div>
                  <div className="settings-text-group">
                    <input
                      type="text"
                      className="settings-text-input"
                      value={sound.startText}
                      maxLength={30}
                      disabled={!sound.enabled}
                      onChange={(e) => props.onSoundChange({ startText: e.target.value })}
                    />
                    <Button
                      size="sm"
                      variant="outline"
                      tone="neutral"
                      disabled={!sound.enabled}
                      onClick={() => props.onPreviewSound('start')}
                    >
                      试听
                    </Button>
                  </div>
                </div>
                <div className="settings-row">
                  <div className="settings-row-main">
                    <span className="settings-row-title">结束语句</span>
                    <span className="settings-row-desc">全局开关停用时朗读</span>
                  </div>
                  <div className="settings-text-group">
                    <input
                      type="text"
                      className="settings-text-input"
                      value={sound.endText}
                      maxLength={30}
                      disabled={!sound.enabled}
                      onChange={(e) => props.onSoundChange({ endText: e.target.value })}
                    />
                    <Button
                      size="sm"
                      variant="outline"
                      tone="neutral"
                      disabled={!sound.enabled}
                      onClick={() => props.onPreviewSound('end')}
                    >
                      试听
                    </Button>
                  </div>
                </div>
              </div>
            </SettingsSection>

            <SettingsSection title="合成参数">
              <div className={`settings-sound-body${sound.enabled ? '' : ' settings-sound-body--disabled'}`}>
                <div className="settings-row">
                  <div className="settings-row-main">
                    <span className="settings-row-title">语音</span>
                  </div>
                  <select
                    className="settings-select"
                    value={sound.voiceName}
                    disabled={!sound.enabled}
                    onChange={(e) => props.onSoundChange({ voiceName: e.target.value })}
                  >
                    {props.availableVoices.length === 0 ? (
                      <option value="">系统无可用语音</option>
                    ) : (
                      <>
                        <option value="">系统默认</option>
                        {props.availableVoices.map((v) => (
                          <option key={v} value={v}>
                            {v}
                          </option>
                        ))}
                      </>
                    )}
                  </select>
                </div>

                <SliderRow
                  label="语速"
                  value={sound.rate}
                  min={-10}
                  max={10}
                  onChange={(v) => props.onSoundChange({ rate: v })}
                  formatValue={(v) => (v === 0 ? '0' : v > 0 ? `+${v}` : `${v}`)}
                />
                <SliderRow
                  label="音调"
                  value={sound.pitch}
                  min={-10}
                  max={10}
                  onChange={(v) => props.onSoundChange({ pitch: v })}
                  formatValue={(v) => (v === 0 ? '0' : v > 0 ? `+${v}` : `${v}`)}
                />
                <SliderRow
                  label="音量"
                  value={sound.volume}
                  min={0}
                  max={100}
                  onChange={(v) => props.onSoundChange({ volume: v })}
                  formatValue={(v) => `${v}%`}
                />
              </div>
            </SettingsSection>
          </>
        )}

        {tab === 'profiles' && (
          <>
            <SettingsSection title="我的配置">
              <div className="settings-profile-toolbar">
                <span>{props.profileCount} 个配置</span>
                <div className="settings-profile-toolbar-actions">
                  <Button
                    variant="outline"
                    tone="primary"
                    size="sm"
                    onClick={props.onCreateProfile}
                  >
                    新建配置
                  </Button>
                  <Button
                    variant="outline"
                    tone="neutral"
                    size="sm"
                    onClick={props.onImportProfile}
                  >
                    导入配置
                  </Button>
                </div>
              </div>

              <ProfileCardList
                profiles={props.profiles}
                activeProfileName={props.profileName}
                defaultProfileName={DEFAULT_PROFILE_NAME}
                onSwitchProfile={props.onSwitchProfile}
                onRenameProfile={props.onRenameProfile}
                onDeleteProfile={props.onDeleteProfile}
              />

              {props.profiles.length === 0 && (
                <p className="settings-note">当前没有配置文件，可先新建或导入配置。</p>
              )}

              {props.isDefaultProfile && (
                <p className="settings-note">默认配置会在修改规则或热键时自动另存为新配置。</p>
              )}
            </SettingsSection>
          </>
        )}
      </div>
    </DialogShell>
  );
}
