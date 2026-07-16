import Button from '../components/Button';
import { CardList, CardListItem } from '../components/CardList';
import { EditIcon } from '../components/icons';
import { keyLabel, type KeyId } from '../components/KeyCapture';
import './ProfileCardList.css';

export interface ProfileMeta {
  name: string;
  created_at: number;
  updated_at: number;
  app_version: string;
}

export interface ProfileSummary {
  rules_total: number;
  rules_enabled: number;
  hold_count: number;
  toggle_count: number;
  group_count: number;
  global_toggle: KeyId | null;
  global_stop: KeyId | null;
  panel_toggle: KeyId | null;
}

export interface SettingsProfileEntry {
  meta: ProfileMeta;
  path: string;
  summary: ProfileSummary;
}

interface Props {
  profiles: SettingsProfileEntry[];
  activeProfileName: string;
  defaultProfileName: string;
  onSwitchProfile: (path: string) => void;
  onRenameProfile: (name: string) => void;
  onDeleteProfile: (name: string) => void;
  onExportProfile: (name: string) => void;
}

function displayProfileName(name: string, defaultProfileName: string): string {
  return name === defaultProfileName ? '默认配置' : name;
}

function ProfileFlag({ tone, children }: { tone: 'on' | 'off'; children: string }) {
  return <span className={`profile-card-flag profile-card-flag--${tone}`}>{children}</span>;
}

function ProfileHotkeys({ summary }: { summary: ProfileSummary }) {
  const stopLabel = summary.global_stop
    ? keyLabel(summary.global_stop)
    : summary.global_toggle
      ? '同开启'
      : '—';

  return (
    <dl className="profile-card-hotkeys">
      <div>
        <dt>全局</dt>
        <dd>{keyLabel(summary.global_toggle)}</dd>
      </div>
      <div>
        <dt>停止</dt>
        <dd>{stopLabel}</dd>
      </div>
      <div>
        <dt>面板</dt>
        <dd>{keyLabel(summary.panel_toggle)}</dd>
      </div>
    </dl>
  );
}

export default function ProfileCardList(props: Props) {
  return (
    <CardList className="profile-card-list">
      {props.profiles.map((profile) => {
        const name = profile.meta.name;
        const displayName = displayProfileName(name, props.defaultProfileName);
        const isActive = name === props.activeProfileName;
        const isDefault = name === props.defaultProfileName;

        return (
          <CardListItem
            key={profile.path}
            className={`profile-card${isDefault ? ' profile-card--default' : ''}`}
            active={isActive}
            interactive={!isActive}
            role="button"
            tabIndex={isActive ? -1 : 0}
            aria-current={isActive ? 'true' : undefined}
            aria-label={`切换到${displayName}`}
            onClick={() => {
              if (!isActive) props.onSwitchProfile(profile.path);
            }}
            onKeyDown={(e) => {
              if (isActive || (e.key !== 'Enter' && e.key !== ' ')) return;
              e.preventDefault();
              props.onSwitchProfile(profile.path);
            }}
          >
            <div className="profile-card-main">
              <span className="profile-card-head">
                <span className="profile-card-name">
                  {displayName}
                  {!isDefault && (
                    <button
                      type="button"
                      className="profile-card-rename-icon"
                      title="重命名"
                      aria-label={`重命名${displayName}`}
                      onClick={(e) => {
                        e.stopPropagation();
                        props.onRenameProfile(name);
                      }}
                    >
                      <EditIcon size={13} />
                    </button>
                  )}
                </span>
                <span className="profile-card-flags">
                  {isActive && <ProfileFlag tone="on">当前</ProfileFlag>}
                  {isDefault && <ProfileFlag tone="off">默认</ProfileFlag>}
                </span>
              </span>
              <span className="profile-card-stats">
                <span>
                  <strong>{profile.summary.hold_count}</strong>
                  按压
                </span>
                <span>
                  <strong>{profile.summary.toggle_count}</strong>
                  切换
                </span>
                {profile.summary.group_count > 0 && (
                  <span>
                    <strong>{profile.summary.group_count}</strong>
                    互斥组
                  </span>
                )}
                <span>
                  <strong>{profile.summary.rules_enabled}</strong>/{profile.summary.rules_total}{' '}
                  启用
                </span>
              </span>
              <ProfileHotkeys summary={profile.summary} />
            </div>
            <div className="profile-card-actions">
              <Button
                variant="outline"
                tone="neutral"
                size="sm"
                onClick={(e) => {
                  e.stopPropagation();
                  props.onExportProfile(name);
                }}
              >
                导出
              </Button>
              {!isDefault && (
                <Button
                  variant="solid"
                  tone="danger"
                  size="sm"
                  className="profile-card-delete-btn"
                  onClick={(e) => {
                    e.stopPropagation();
                    props.onDeleteProfile(name);
                  }}
                >
                  删除
                </Button>
              )}
            </div>
          </CardListItem>
        );
      })}
    </CardList>
  );
}
