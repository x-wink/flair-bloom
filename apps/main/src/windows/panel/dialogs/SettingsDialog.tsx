import { type ReactNode, useState } from 'react';
import { APP_NAME } from '../../../constants';
import Button from '../components/Button';
import { CardList, CardListButton } from '../components/CardList';
import type { CloseBehavior } from '../components/CloseBehaviorForm';
import ProfileCardList, { type SettingsProfileEntry } from './ProfileCardList';
import './dialog-base.css';
import './SettingsDialog.css';

export type SettingsTab = 'general' | 'profiles';
type SettingsInputMode = 'sendinput' | 'interception' | 'dd_hid';
type DriverStatus = 'installed' | 'pending_reboot' | 'not_installed';

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
  profiles: SettingsProfileEntry[];
  profileName: string;
  profileCount: number;
  isDefaultProfile: boolean;
  onClose: () => void;
  onSelectInputMode: (mode: SettingsInputMode) => void;
  onToggleGlobal: () => void;
  onSetCloseBehavior: (choice: CloseBehavior | null) => void;
  onCreateProfile: () => void;
  onImportProfile: () => void;
  onSwitchProfile: (path: string) => void;
  onRenameProfile: (name: string) => void;
  onDeleteProfile: (name: string) => void;
}

const TABS: { id: SettingsTab; label: string }[] = [
  { id: 'general', label: '通用' },
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

export default function SettingsDialog(props: Props) {
  const [tab, setTab] = useState<SettingsTab>(props.initialTab ?? 'general');
  const versionLabel = props.appVersion ? `v${props.appVersion}` : '版本加载中';

  return (
    <div className="settings-card" role="dialog" aria-modal="true" aria-labelledby="settings-title">
      <header className="settings-header">
        <div>
          <h2 id="settings-title" className="settings-title">
            设置
          </h2>
          <p className="settings-subtitle">
            {APP_NAME} {versionLabel}
          </p>
        </div>
        <Button size="sm" variant="ghost" tone="neutral" onClick={props.onClose}>
          关闭
        </Button>
      </header>

      <nav className="settings-tabs" aria-label="设置分区">
        {TABS.map((item) => (
          <button
            key={item.id}
            type="button"
            className={`settings-tab${tab === item.id ? ' settings-tab--active' : ''}`}
            aria-selected={tab === item.id}
            onClick={() => setTab(item.id)}
          >
            {item.label}
          </button>
        ))}
      </nav>

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
    </div>
  );
}
