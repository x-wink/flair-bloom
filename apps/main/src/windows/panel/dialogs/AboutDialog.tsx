import './AboutDialog.css';

interface Props {
  version: string;
  onClose: () => void;
}

export default function AboutDialog({ version, onClose }: Props) {
  return (
    <div className="about-card">
      <p className="about-title">
        气质花按键助手<span className="about-ver">{version ? `v${version}` : '版本加载中…'}</span>
      </p>
      <p className="about-desc">加强花椒油！！！加强紫武区！！！</p>
      <button className="btn-primary about-close-btn" onClick={onClose}>
        我同意
      </button>
    </div>
  );
}
