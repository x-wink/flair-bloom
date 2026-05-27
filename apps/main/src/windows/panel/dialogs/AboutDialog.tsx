import { useState } from 'react';
import { APP_NAME } from '../../../constants';
import Button from '../components/Button';
import './dialog-base.css';
import './AboutDialog.css';

type DriverStatus = 'installed' | 'pending_reboot' | 'not_installed';

export interface AboutDialogInfo {
  appVersion: string;
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
  global_enabled: boolean;
  rules_total: number;
  rules_enabled: number;
  active_rule_ids: string[];
}

interface Props {
  info: AboutDialogInfo;
  onClose: () => void;
  onUninstallInterception: () => void;
  onUninstallDdHid: () => void;
  onOpenDir: (kind: 'install' | 'data' | 'log' | 'drivers') => void;
  onCopied: () => void;
  onCopyFailed: (err: unknown) => void;
}

const INPUT_MODE_LABEL: Record<string, string> = {
  sendinput: '通用模式',
  interception: '游戏模式',
  dd_hid: '究极HID',
};

function formatInputMode(mode: string): string {
  return INPUT_MODE_LABEL[mode] ?? mode;
}

function YesNo({ value, yes = '是', no = '否' }: { value: boolean; yes?: string; no?: string }) {
  return (
    <span className={`about-flag ${value ? 'about-flag--on' : 'about-flag--off'}`}>
      {value ? yes : no}
    </span>
  );
}

function DriverFlag({ status }: { status: DriverStatus }) {
  // 待重启用 warn 色：既非"已生效"，也非"完全没装"，提醒用户重启而不是再点一次安装
  const cls =
    status === 'installed'
      ? 'about-flag--on'
      : status === 'pending_reboot'
        ? 'about-flag--warn'
        : 'about-flag--off';
  const label =
    status === 'installed' ? '已安装' : status === 'pending_reboot' ? '待重启' : '未安装';
  return <span className={`about-flag ${cls}`}>{label}</span>;
}

export default function AboutDialog({
  info,
  onClose,
  onUninstallInterception,
  onUninstallDdHid,
  onOpenDir,
  onCopied,
  onCopyFailed,
}: Props) {
  const [copying, setCopying] = useState(false);

  async function handleCopy() {
    if (copying) return;
    setCopying(true);
    try {
      const payload = {
        app: { name: APP_NAME, version: info.appVersion },
        elevated: info.elevated,
        interception_installed: info.interception_installed,
        dd_hid_installed: info.dd_hid_installed,
        input_mode: info.input_mode,
        platform: info.platform,
        os_family: info.os_family,
        os_version: info.os_version,
        webview_version: info.webview_version,
        arch: info.arch,
        locale: info.locale,
        install_path: info.install_path,
        log_dir: info.log_dir,
        app_data_dir: info.app_data_dir,
        autostart_enabled: info.autostart_enabled,
        resources_ok: info.resources_ok,
        missing_resources: info.missing_resources,
        global_enabled: info.global_enabled,
        rules_total: info.rules_total,
        rules_enabled: info.rules_enabled,
        active_rule_ids: info.active_rule_ids,
      };
      await navigator.clipboard.writeText(JSON.stringify(payload, null, 2));
      onCopied();
    } catch (e) {
      onCopyFailed(e);
    } finally {
      setCopying(false);
    }
  }

  const webviewMissing = !info.webview_version;

  return (
    <div className="about-card">
      <header className="about-header">
        <h2 className="about-title">
          {APP_NAME}
          <span className="about-ver">
            {info.appVersion ? `v${info.appVersion}` : '版本加载中…'}
          </span>
        </h2>
        <p className="about-tagline">加强花椒油！！！加强紫武区！！！</p>
        {!info.resources_ok && (
          <p className="about-warn-banner">
            检测到缺失资源：{info.missing_resources.join('、') || '未知'}。
            可能被杀毒软件误删，建议重新安装应用。
          </p>
        )}
      </header>

      <div className="about-body">
        <section className="about-section">
          <p className="about-section-label">系统环境</p>
          <ul className="about-list">
            <li>
              <span className="about-key">操作系统</span>
              <span className="about-value">
                {info.os_version || info.platform || '—'}
                {info.os_family && info.os_family !== info.platform ? ` (${info.os_family})` : ''}
              </span>
            </li>
            <li>
              <span className="about-key">主机架构</span>
              <span className="about-value">{info.arch || '—'}</span>
            </li>
            <li>
              <span className="about-key">语言区域</span>
              <span className="about-value">{info.locale || '—'}</span>
            </li>
            <li>
              <span className="about-key">WebView2</span>
              <span className="about-value">
                {webviewMissing ? (
                  <span className="about-flag about-flag--warn">未检测到</span>
                ) : (
                  info.webview_version
                )}
              </span>
            </li>
            <li>
              <span className="about-key">管理员启动</span>
              <span className="about-value">
                <YesNo value={info.elevated} />
              </span>
            </li>
            <li>
              <span className="about-key">开机自启</span>
              <span className="about-value">
                <YesNo value={info.autostart_enabled} yes="已启用" no="未启用" />
              </span>
            </li>
            <li>
              <span className="about-key">安装目录</span>
              <span className="about-value about-value--with-action">
                <Button size="sm" variant="outline" onClick={() => onOpenDir('install')}>
                  打开
                </Button>
              </span>
            </li>
            <li>
              <span className="about-key">数据目录</span>
              <span className="about-value about-value--with-action">
                <Button size="sm" variant="outline" onClick={() => onOpenDir('data')}>
                  打开
                </Button>
              </span>
            </li>
            <li>
              <span className="about-key">日志目录</span>
              <span className="about-value about-value--with-action">
                <Button size="sm" variant="outline" onClick={() => onOpenDir('log')}>
                  打开
                </Button>
              </span>
            </li>
            {info.os_family === 'windows' && (
              <li>
                <span className="about-key">驱动目录</span>
                <span className="about-value about-value--with-action">
                  <Button size="sm" variant="outline" onClick={() => onOpenDir('drivers')}>
                    打开
                  </Button>
                </span>
              </li>
            )}
          </ul>
        </section>

        <section className="about-section">
          <p className="about-section-label">输入引擎</p>
          <ul className="about-list">
            <li>
              <span className="about-key">当前模式</span>
              <span className="about-value">{formatInputMode(info.input_mode)}</span>
            </li>
            <li>
              <span className="about-key">游戏模式驱动</span>
              <span className="about-value about-value--with-action">
                <DriverFlag status={info.interception_installed} />
                {info.interception_installed === 'installed' && (
                  <Button
                    size="sm"
                    variant="outline"
                    tone="danger"
                    onClick={onUninstallInterception}
                  >
                    卸载
                  </Button>
                )}
              </span>
            </li>
            <li>
              <span className="about-key">究极HID 驱动</span>
              <span className="about-value about-value--with-action">
                <DriverFlag status={info.dd_hid_installed} />
                {info.dd_hid_installed === 'installed' && (
                  <Button size="sm" variant="outline" tone="danger" onClick={onUninstallDdHid}>
                    卸载
                  </Button>
                )}
              </span>
            </li>
          </ul>
        </section>

        <section className="about-section">
          <p className="about-section-label">连发引擎</p>
          <ul className="about-list">
            <li>
              <span className="about-key">全局开关</span>
              <span className="about-value">
                <YesNo value={info.global_enabled} yes="已启用" no="已停用" />
              </span>
            </li>
            <li>
              <span className="about-key">启用规则</span>
              <span className="about-value">
                {info.rules_enabled} / {info.rules_total}
              </span>
            </li>
            <li>
              <span className="about-key">激活规则</span>
              <span className="about-value">
                {info.active_rule_ids.length === 0
                  ? '无'
                  : `${info.active_rule_ids.length} 条运行中`}
              </span>
            </li>
          </ul>
        </section>
      </div>

      <div className="about-actions">
        <Button variant="outline" onClick={handleCopy} loading={copying}>
          复制信息
        </Button>
        <Button onClick={onClose}>关闭</Button>
      </div>
    </div>
  );
}
