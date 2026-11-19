import type { LiveEnvironment } from '../environment';
import Button from '../components/Button';
import './AboutDialog.css';
const INPUT_MODE_LABELS: Record<string, string> = {
  sendinput: '轻量模式',
  interception: '游戏模式',
  ddsimple: '网吧模式',
  hidstick: '外设模式',
};

export interface SystemInfo {
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

export interface EnvironmentProps {
  info: SystemInfo;
  appearance: { layout: string; scale: number; maximized: boolean };
  onOpenDir: (kind: 'install' | 'data' | 'log' | 'drivers') => void;
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

export default function SystemEnvironment({
  info,
  appearance,
  environment,
  onOpenDir,
}: EnvironmentProps & { environment: LiveEnvironment | null }) {
  const runtime = environment?.status;
  const display = environment?.display;
  const panel = display?.windows.find((w) => w.label === 'panel');
  const float = display?.windows.find((w) => w.label === 'float');
  const webviewVersion =
    display?.runtime_version || runtime?.webview_version || info.webview_version;
  const percent = (factor: number) => `${Math.round(factor * 1000) / 10}%`;
  const size = (value: { width: number; height: number } | null | undefined) =>
    value ? `${value.width} × ${value.height}` : '未取得';

  return (
    <>
      <section className="about-section">
        <p className="about-section-label">运行环境</p>
        <ul className="about-list">
          <InfoRow
            label="操作系统"
            value={
              <>
                {runtime?.os_version || info.os_version || info.platform || '—'}
                {info.os_family && info.os_family !== info.platform ? ` (${info.os_family})` : ''}
              </>
            }
          />
          {!(runtime ?? info).resources_ok && (
            <InfoRow label="缺失资源" value={(runtime ?? info).missing_resources.join('、')} />
          )}
          <InfoRow label="主机架构" value={info.arch || '—'} />
          <InfoRow label="语言区域" value={info.locale || '—'} />
          <InfoRow
            label="高精度计时"
            value={(runtime ?? info).scheduler_hp_degraded ? '已降级' : '可用'}
          />
          <InfoRow
            label="管理员权限"
            value={runtime ? (runtime.elevated ? '已获得' : '未获得') : '未取得'}
          />
          <InfoRow
            label="当前输入通道"
            value={runtime ? INPUT_MODE_LABELS[runtime.input_mode] || runtime.input_mode : '未取得'}
          />
          <InfoRow
            label="快速启动配置"
            value={
              display?.fast_startup_configured == null
                ? '未取得'
                : display.fast_startup_configured
                  ? '已开启'
                  : '已关闭'
            }
          />
          <InfoRow
            label="WebView2"
            value={
              !webviewVersion ? (
                <span className="about-flag about-flag--warn">未检测到</span>
              ) : (
                webviewVersion
              )
            }
          />
        </ul>
      </section>

      <section className="about-section">
        <p className="about-section-label">显示环境</p>
        <ul className="about-list">
          <InfoRow
            label="系统文字大小"
            value={
              display?.text_scale_percent != null
                ? `${display.text_scale_percent}%（应用忽略）`
                : '未取得'
            }
          />
          <InfoRow
            label="应用布局 / 缩放"
            value={`${appearance.layout === 'horizontal' ? '横版' : '竖版'} · ${percent(appearance.scale)}${appearance.maximized ? ' · 最大化' : ''}`}
          />
          <InfoRow
            label="当前屏幕缩放"
            value={panel?.scale_factor != null ? percent(panel.scale_factor) : '未取得'}
          />
          <InfoRow
            label="面板 / 浮窗尺寸"
            value={`${size(panel?.client_physical)} / ${size(float?.client_physical)} px`}
          />
          <InfoRow label="页面视口" value={`${size(environment?.frontend.viewport_css)} CSS px`} />
          <InfoRow
            label="页面像素比"
            value={environment ? String(environment.frontend.device_pixel_ratio) : '未取得'}
          />
          <InfoRow
            label="WebView 实际缩放"
            value={panel?.webview ? percent(panel.webview.zoom_factor) : '未取得'}
          />
          <InfoRow
            label="WebView 像素缩放"
            value={panel?.webview ? percent(panel.webview.rasterization_scale) : '未取得'}
          />
          {display?.monitors.map((monitor, index) => (
            <InfoRow
              key={`${monitor.name}-${index}`}
              label={`显示器 ${index + 1}${monitor.primary ? '（主）' : ''}`}
              value={`${size(monitor.size_physical)} · ${percent(monitor.scale_factor)}`}
            />
          ))}
        </ul>
        {!!(environment?.errors.length || display?.errors.length) && (
          <p className="about-warn-banner">部分环境信息未取得，错误详情会写入诊断报告。</p>
        )}
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
    </>
  );
}
