import { useState } from 'react';
import { APP_NAME, AUTHOR_EMAIL } from '../../../constants';
import Button from '../components/Button';
import DialogShell from './DialogShell';
import './AboutDialog.css';

export interface AboutDialogInfo {
  appVersion: string;
  platform: string;
  os_family: string;
  os_version: string;
  webview_version: string;
  arch: string;
  locale: string;
  install_path: string;
  log_dir: string;
  app_data_dir: string;
  resources_ok: boolean;
  missing_resources: string[];
  scheduler_hp_degraded: boolean;
}

interface Props {
  info: AboutDialogInfo;
  onClose: () => void;
  onWriteMail: () => void;
  onOpenDir: (kind: 'install' | 'data' | 'log' | 'drivers') => void;
  onCopied: () => void;
  onCopyFailed: (err: unknown) => void;
}

function InfoRow({ label, value }: { label: string; value: string | React.ReactNode }) {
  return (
    <li>
      <span className="about-key">{label}</span>
      <span className="about-value">{value}</span>
    </li>
  );
}

function DirRow({ label, onOpen }: { label: string; onOpen: () => void }) {
  return (
    <li>
      <span className="about-key">{label}</span>
      <span className="about-value about-value--with-action">
        <Button size="sm" variant="outline" onClick={onOpen}>
          打开
        </Button>
      </span>
    </li>
  );
}

export default function AboutDialog({
  info,
  onClose,
  onWriteMail,
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
        platform: info.platform,
        os_family: info.os_family,
        os_version: info.os_version,
        webview_version: info.webview_version,
        arch: info.arch,
        locale: info.locale,
        install_path: info.install_path,
        log_dir: info.log_dir,
        app_data_dir: info.app_data_dir,
        resources_ok: info.resources_ok,
        missing_resources: info.missing_resources,
        scheduler_hp_degraded: info.scheduler_hp_degraded,
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

  const headerNode = (
    <>
      <h2 className="about-title">
        {APP_NAME}
        <span className="about-ver">{info.appVersion ? `v${info.appVersion}` : '版本加载中…'}</span>
      </h2>
      <p className="about-tagline">加强花椒油！！！加强紫武区！！！</p>
      {!info.resources_ok && (
        <p className="about-warn-banner">
          检测到缺失资源：{info.missing_resources.join('、') || '未知'}。
          可能被杀毒软件误删，建议重新安装应用。
        </p>
      )}
    </>
  );

  const footerNode = (
    <>
      <Button variant="outline" onClick={handleCopy} loading={copying}>
        复制环境信息
      </Button>
      <Button onClick={onClose}>关闭</Button>
    </>
  );

  return (
    <DialogShell className="about-card" headerContent={headerNode} footer={footerNode}>
      <div className="about-body">
        <section className="about-section">
          <p className="about-section-label">联系作者</p>
          <ul className="about-list">
            <li>
              <span className="about-key">邮箱</span>
              <span className="about-value about-value--with-action">
                <span className="about-email">{AUTHOR_EMAIL}</span>
                <Button size="sm" variant="outline" tone="primary" onClick={onWriteMail}>
                  写邮件
                </Button>
              </span>
            </li>
          </ul>
        </section>

        <section className="about-section">
          <p className="about-section-label">运行环境</p>
          <ul className="about-list">
            <InfoRow
              label="操作系统"
              value={
                <>
                  {info.os_version || info.platform || '—'}
                  {info.os_family && info.os_family !== info.platform ? ` (${info.os_family})` : ''}
                </>
              }
            />
            <InfoRow label="主机架构" value={info.arch || '—'} />
            <InfoRow label="语言区域" value={info.locale || '—'} />
            <InfoRow label="高精度计时" value={info.scheduler_hp_degraded ? '已降级' : '可用'} />
            <InfoRow
              label="WebView2"
              value={
                webviewMissing ? (
                  <span className="about-flag about-flag--warn">未检测到</span>
                ) : (
                  info.webview_version
                )
              }
            />
          </ul>
        </section>

        <section className="about-section">
          <p className="about-section-label">目录</p>
          <ul className="about-list">
            <DirRow label="安装目录" onOpen={() => onOpenDir('install')} />
            <DirRow label="数据目录" onOpen={() => onOpenDir('data')} />
            <DirRow label="日志目录" onOpen={() => onOpenDir('log')} />
            {info.os_family === 'windows' && (
              <DirRow label="驱动目录" onOpen={() => onOpenDir('drivers')} />
            )}
          </ul>
        </section>
      </div>
    </DialogShell>
  );
}
