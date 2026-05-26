import Button from '../components/Button';
import './dialog-base.css';
import './UpdateNoticeDialog.css';

export interface UpdateNoticeInfo {
  version: string;
  notes: string | null;
}

interface Props {
  info: UpdateNoticeInfo;
  onClose: () => void;
}

export default function UpdateNoticeDialog({ info, onClose }: Props) {
  return (
    <div className="update-notice-card">
      <div className="update-notice-header">
        <span className="update-badge">新版本</span>
        <h2 className="update-notice-title">v{info.version}</h2>
        <p className="update-notice-hint">更新已下载完成，重启应用后生效</p>
      </div>

      {info.notes && (
        <div className="update-notes-section">
          <p className="update-notes-label">更新内容</p>
          <div className="update-notes-body">
            <pre className="update-notes-text">{info.notes}</pre>
          </div>
        </div>
      )}

      <div className="update-notice-actions">
        <Button className="update-notice-close" onClick={onClose}>
          知道了
        </Button>
      </div>
    </div>
  );
}
