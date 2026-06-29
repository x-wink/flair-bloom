import { type ReactNode, useState } from 'react';
import Button from '../components/Button';
import { CardList, CardListButton } from '../components/CardList';
import type { CloseBehavior } from '../components/CloseBehaviorForm';
import KeyCapture, { type KeyId } from '../components/KeyCapture';
import { VolumeIcon } from '../components/icons';
import Tabs from '../components/Tabs';
import type { ConflictSeverity } from '../conflicts';
import DialogShell from './DialogShell';
import ProfileCardList, { type SettingsProfileEntry } from './ProfileCardList';
import './SettingsDialog.css';

export type SettingsTab = 'general' | 'hotkeys' | 'sound' | 'profiles';
type SettingsInputMode = 'sendinput' | 'interception' | 'ddsimple' | 'dd_hid';
type DriverStatus = 'installed' | 'pending_reboot' | 'not_installed';

export interface SoundSettings {
  enabled: boolean;
  // 每个播报时机独立开关，默认开；总开关 enabled 关闭时整体静音
  startEnabled: boolean;
  endEnabled: boolean;
  toggleStartEnabled: boolean;
  toggleEndEnabled: boolean;
  volume: number;
  rate: number;
  pitch: number;
  startText: string;
  endText: string;
  toggleStartText: string;
  toggleEndText: string;
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
  elevated: boolean;
  closeBehavior: CloseBehavior | null;
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
  onPreviewSound: (type: 'start' | 'end' | 'toggleStart' | 'toggleEnd') => void;
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
  ddsimple: 'DD驱动',
  dd_hid: 'DDHID',
};

const INPUT_MODE_HINTS: Record<SettingsInputMode, string> = {
  sendinput: 'SendInput',
  interception: 'Interception',
  ddsimple: 'DD驱动',
  dd_hid: 'DDHID',
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
    if (props.interceptionInstalled !== 'installed') {
      return driverStatusLabel(props.interceptionInstalled);
    }
    return props.elevated ? '管理员已就绪' : '需要管理员';
  }
  if (mode === 'ddsimple') {
    return props.elevated ? '管理员已就绪' : '需要管理员';
  }
  return '无需驱动';
}

/// 模式角标：游戏模式主推（推荐）、DD驱动降级（备用）。通用模式无角标。
function modeTag(mode: SettingsInputMode): { text: string; kind: 'recommend' | 'backup' } | null {
  if (mode === 'interception') return { text: '推荐', kind: 'recommend' };
  if (mode === 'ddsimple') return { text: '备用', kind: 'backup' };
  return null;
}

/// 可选输入模式：游戏模式置顶主推，通用模式次之，DD驱动仅作备用。DDHID 已禁用、不列出
/// （后端上报 dd_hid 会被自动回退为通用模式，故界面永不停留在 DDHID）。
const SELECTABLE_INPUT_MODES: SettingsInputMode[] = ['interception', 'sendinput', 'ddsimple'];

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

function hotkeyKeyEq(a: KeyId | null, b: KeyId | null): boolean {
  return !!a && !!b && a.kind === b.kind && a.code === b.code;
}

// 单条播报语句行：文本框（含内嵌试听图标）+ 行尾独立开关。
// 文本编辑与试听仅受总开关 masterEnabled 限制；行尾开关仅控制实际播报，不锁定编辑。
function SoundStatementRow({
  title,
  desc,
  value,
  enabled,
  masterEnabled,
  onTextChange,
  onToggle,
  onPreview,
}: {
  title: string;
  desc: ReactNode;
  value: string;
  enabled: boolean;
  masterEnabled: boolean;
  onTextChange: (v: string) => void;
  onToggle: (on: boolean) => void;
  onPreview: () => void;
}) {
  return (
    <div className="settings-row">
      <div className="settings-row-main">
        <span className="settings-row-title">{title}</span>
        <span className="settings-row-desc">{desc}</span>
      </div>
      <div className="settings-text-group">
        <div className="settings-input-wrap">
          <input
            type="text"
            className="settings-text-input"
            value={value}
            maxLength={30}
            disabled={!masterEnabled}
            onChange={(e) => onTextChange(e.target.value)}
          />
          <button
            type="button"
            className="settings-input-icon-btn"
            disabled={!masterEnabled}
            onClick={onPreview}
            aria-label="试听"
            title="试听"
          >
            <VolumeIcon />
          </button>
        </div>
        <input
          type="checkbox"
          className="enable-checkbox"
          checked={enabled}
          disabled={!masterEnabled}
          onChange={(e) => onToggle(e.target.checked)}
          aria-label={`${title}提示开关`}
        />
      </div>
    </div>
  );
}

export default function SettingsDialog(props: Props) {
  const [tab, setTab] = useState<SettingsTab>(props.initialTab ?? 'general');
  const [hotkeyDupNote, setHotkeyDupNote] = useState<string | null>(null);
  const { sound } = props;

  // 全局热键只能绑定键盘实体键（KeyCapture keyboardOnly），且三者互不重复——重复会让该键被
  // 某个热键抢先处理、其余功能失效，行为不可预期。绑定前若与另一全局热键相同则拒绝并提示。
  const setGlobalHotkey = (
    field: 'global_toggle' | 'global_stop' | 'panel_toggle',
    key: KeyId | null,
  ) => {
    if (key) {
      const others = (['global_toggle', 'global_stop', 'panel_toggle'] as const).filter(
        (f) => f !== field,
      );
      if (others.some((f) => hotkeyKeyEq(props.hotkeys[f], key))) {
        setHotkeyDupNote('该按键已被其它全局热键占用，请换一个');
        return;
      }
    }
    setHotkeyDupNote(null);
    if (field === 'global_toggle') {
      props.onHotkeyChange({
        global_toggle: key,
        global_stop: key === null ? null : props.hotkeys.global_stop,
      });
    } else if (field === 'global_stop') {
      props.onHotkeyChange({ global_stop: key });
    } else {
      props.onHotkeyChange({ panel_toggle: key });
    }
  };

  const tabsNode = <Tabs tabs={TABS} active={tab} onChange={setTab} variant="pill" grow />;

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
                {SELECTABLE_INPUT_MODES.map((mode) => {
                  const tag = modeTag(mode);
                  return (
                    <CardListButton
                      key={mode}
                      active={props.inputMode === mode}
                      className="settings-mode"
                      disabled={props.switchingMode}
                      onClick={() => props.onSelectInputMode(mode)}
                    >
                      <span className="settings-mode-name">
                        {INPUT_MODE_LABELS[mode]}
                        {tag && (
                          <span className={`settings-mode-tag settings-mode-tag-${tag.kind}`}>
                            {tag.text}
                          </span>
                        )}
                      </span>
                      <span className="settings-mode-meta">
                        {INPUT_MODE_HINTS[mode]} · {modeDetail(mode, props)}
                      </span>
                    </CardListButton>
                  );
                })}
              </CardList>
              <p className="settings-note settings-note-warn">
                ⚠️ DD 驱动（含 DDHID）可能无法正确停止连发、甚至自行停止连发，已不再推荐。DDHID
                已禁用；DD驱动仅在「游戏模式」不可用时作为备用。优先使用游戏模式。
              </p>
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
                    keyboardOnly
                    placeholder="未设置"
                    conflict={props.hotkeyConflicts.global_toggle}
                    onChange={(k) => setGlobalHotkey('global_toggle', k)}
                  />
                  {props.hotkeys.global_toggle && (
                    <>
                      <span className="settings-hotkey-sep">停止</span>
                      <KeyCapture
                        value={props.hotkeys.global_stop}
                        nullable
                        keyboardOnly
                        placeholder="同开启键"
                        conflict={props.hotkeyConflicts.global_stop}
                        onChange={(k) => setGlobalHotkey('global_stop', k)}
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
                    keyboardOnly
                    placeholder="未设置"
                    conflict={props.hotkeyConflicts.panel_toggle}
                    onChange={(k) => setGlobalHotkey('panel_toggle', k)}
                  />
                </div>
              </div>
            </div>
            {hotkeyDupNote && <p className="settings-note settings-note-warn">{hotkeyDupNote}</p>}
            <p className="settings-note">
              全局热键仅支持键盘按键，三个热键不能重复；热键随当前配置文件保存。
            </p>
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
                <input
                  type="checkbox"
                  className="enable-checkbox"
                  checked={sound.enabled}
                  onChange={(e) => props.onSoundChange({ enabled: e.target.checked })}
                  aria-label="声音反馈"
                />
              </div>
            </SettingsSection>

            <SettingsSection title="语句">
              <div
                className={`settings-sound-body${sound.enabled ? '' : ' settings-sound-body--disabled'}`}
              >
                <SoundStatementRow
                  title="全局启用"
                  desc="全局开关启用时朗读"
                  value={sound.startText}
                  enabled={sound.startEnabled}
                  masterEnabled={sound.enabled}
                  onTextChange={(v) => props.onSoundChange({ startText: v })}
                  onToggle={(on) => props.onSoundChange({ startEnabled: on })}
                  onPreview={() => props.onPreviewSound('start')}
                />
                <SoundStatementRow
                  title="全局停用"
                  desc="全局开关停用时朗读"
                  value={sound.endText}
                  enabled={sound.endEnabled}
                  masterEnabled={sound.enabled}
                  onTextChange={(v) => props.onSoundChange({ endText: v })}
                  onToggle={(on) => props.onSoundChange({ endEnabled: on })}
                  onPreview={() => props.onPreviewSound('end')}
                />
                <SoundStatementRow
                  title="Toggle 开始"
                  desc={<>启动时朗读，{'${key}'} 替换为目标键名</>}
                  value={sound.toggleStartText}
                  enabled={sound.toggleStartEnabled}
                  masterEnabled={sound.enabled}
                  onTextChange={(v) => props.onSoundChange({ toggleStartText: v })}
                  onToggle={(on) => props.onSoundChange({ toggleStartEnabled: on })}
                  onPreview={() => props.onPreviewSound('toggleStart')}
                />
                <SoundStatementRow
                  title="Toggle 结束"
                  desc={<>停止时朗读，{'${key}'} 替换为目标键名</>}
                  value={sound.toggleEndText}
                  enabled={sound.toggleEndEnabled}
                  masterEnabled={sound.enabled}
                  onTextChange={(v) => props.onSoundChange({ toggleEndText: v })}
                  onToggle={(on) => props.onSoundChange({ toggleEndEnabled: on })}
                  onPreview={() => props.onPreviewSound('toggleEnd')}
                />
              </div>
            </SettingsSection>

            <SettingsSection title="合成参数">
              <div
                className={`settings-sound-body${sound.enabled ? '' : ' settings-sound-body--disabled'}`}
              >
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
