import { useState } from 'react';
import { APP_NAME } from '../../../constants';
import Button from '../components/Button';
import type { UpdateNoticeInfo } from './UpdateNoticeDialog';
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
}

interface Props {
  info: AboutDialogInfo;
  updateNotice: UpdateNoticeInfo | null;
  checkingUpdate: boolean;
  onClose: () => void;
  onCheckUpdate: () => void;
  onShowUpdateNotice: () => void;
  onShowAgreement: () => void;
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
  updateNotice,
  checkingUpdate,
  onClose,
  onCheckUpdate,
  onShowUpdateNotice,
  onShowAgreement,
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

        <section className="about-section">
          <p className="about-section-label">版本与协议</p>
          <ul className="about-list">
            <li>
              <span className="about-key">检查更新</span>
              <span className="about-value about-value--with-action">
                {updateNotice && (
                  <span className="about-flag about-flag--primary">
                    新版本 v{updateNotice.version}
                  </span>
                )}
                <Button
                  size="sm"
                  variant="outline"
                  tone="primary"
                  loading={checkingUpdate}
                  onClick={onCheckUpdate}
                >
                  检查更新
                </Button>
              </span>
            </li>
            <li>
              <span className="about-key">更新公告</span>
              <span className="about-value about-value--with-action">
                {updateNotice && <span className="about-update-dot" aria-hidden="true" />}
                <Button
                  size="sm"
                  variant="outline"
                  disabled={!updateNotice}
                  onClick={onShowUpdateNotice}
                >
                  {updateNotice ? '查看公告' : '暂无公告'}
                </Button>
              </span>
            </li>
            <li>
              <span className="about-key">用户协议</span>
              <span className="about-value about-value--with-action">
                <Button size="sm" variant="outline" onClick={onShowAgreement}>
                  查看
                </Button>
              </span>
            </li>
          </ul>
        </section>
      </div>
    </DialogShell>
  );
}
