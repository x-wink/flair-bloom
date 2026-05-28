import './UpdateProgressBar.css';

export interface UpdateDownloadProgress {
  version: string;
  downloaded: number;
  total: number | null;
  percent: number | null;
  done: boolean;
}

interface Props {
  progress: UpdateDownloadProgress;
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'];
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  const precision = unitIndex === 0 || value >= 10 ? 0 : 1;
  return `${value.toFixed(precision)} ${units[unitIndex]}`;
}

function progressText(progress: UpdateDownloadProgress): string {
  if (progress.done) return '下载完成';
  if (progress.total && progress.total > 0 && progress.percent !== null) {
    return `${Math.round(progress.percent)}% · ${formatBytes(progress.downloaded)} / ${formatBytes(progress.total)}`;
  }
  return `${formatBytes(progress.downloaded)} 已下载`;
}

export default function UpdateProgressBar({ progress }: Props) {
  const percent = progress.percent === null ? null : Math.min(100, Math.max(0, progress.percent));

  return (
    <div
      className={`update-progress${progress.done ? ' done' : ''}${percent === null ? ' indeterminate' : ''}`}
      role="status"
      aria-live="polite"
    >
      <div className="update-progress-info">
        <span className="update-progress-title">正在更新到 v{progress.version}</span>
        <span className="update-progress-value">{progressText(progress)}</span>
      </div>
      <div
        className="update-progress-track"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent === null ? undefined : Math.round(percent)}
      >
        <span
          className="update-progress-fill"
          style={percent === null ? undefined : { width: `${percent}%` }}
        />
      </div>
    </div>
  );
}
