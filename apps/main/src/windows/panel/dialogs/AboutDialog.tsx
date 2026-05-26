import { APP_NAME } from '../../../constants';
import Button from '../components/Button';
import './dialog-base.css';
import './AboutDialog.css';

interface Props {
  version: string;
  onClose: () => void;
}

export default function AboutDialog({ version, onClose }: Props) {
  return (
    <div className="about-card">
      <p className="about-title">
        {APP_NAME}
        <span className="about-ver">{version ? `v${version}` : '版本加载中…'}</span>
      </p>
      <p className="about-desc">加强花椒油！！！加强紫武区！！！</p>
      <Button className="about-close-btn" onClick={onClose}>
        我同意
      </Button>
    </div>
  );
}
