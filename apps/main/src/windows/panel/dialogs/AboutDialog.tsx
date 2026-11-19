import { APP_NAME, AUTHOR_EMAIL, PRODUCT_QQ_GROUP, PRODUCT_WEBSITE } from '../../../constants';
import Button from '../components/Button';
import DialogShell from './DialogShell';
import './AboutDialog.css';

interface Props {
  appVersion: string;
  onClose: () => void;
  onWriteMail: () => void;
  onGroupCopied: () => void;
  onOpenWebsite: () => void;
  onWebsiteCopied: () => void;
  onCopyFailed: (err: unknown) => void;
}

export default function AboutDialog({
  appVersion,
  onClose,
  onWriteMail,
  onGroupCopied,
  onOpenWebsite,
  onWebsiteCopied,
  onCopyFailed,
}: Props) {
  async function handleCopyGroup() {
    try {
      await navigator.clipboard.writeText(PRODUCT_QQ_GROUP);
      onGroupCopied();
    } catch (e) {
      onCopyFailed(e);
    }
  }

  async function handleCopyWebsite() {
    try {
      await navigator.clipboard.writeText(PRODUCT_WEBSITE);
      onWebsiteCopied();
    } catch (e) {
      onCopyFailed(e);
    }
  }

  const headerNode = (
    <>
      <h2 className="about-title">
        {APP_NAME}
        <span className="about-ver">{appVersion ? `v${appVersion}` : '版本加载中…'}</span>
      </h2>
      <p className="about-tagline">加强花椒油！！！加强紫武区！！！</p>
    </>
  );

  const footerNode = (
    <>
      <Button onClick={onClose}>关闭</Button>
    </>
  );

  return (
    <DialogShell className="about-card" headerContent={headerNode} footer={footerNode}>
      <div className="about-body">
        <section className="about-section">
          <p className="about-section-label">交流与联系</p>
          <ul className="about-list">
            <li>
              <span className="about-key">官网</span>
              <span className="about-value about-value--with-action about-website">
                <button type="button" className="about-website-link" onClick={onOpenWebsite}>
                  {PRODUCT_WEBSITE}
                </button>
                <Button size="sm" variant="outline" tone="primary" onClick={handleCopyWebsite}>
                  复制链接
                </Button>
              </span>
            </li>
            <li>
              <span className="about-key">产品QQ群</span>
              <span className="about-value about-value--with-action">
                <span className="about-contact">{PRODUCT_QQ_GROUP}</span>
                <Button size="sm" variant="outline" tone="primary" onClick={handleCopyGroup}>
                  复制群号
                </Button>
              </span>
            </li>
            <li>
              <span className="about-key">邮箱</span>
              <span className="about-value about-value--with-action">
                <span className="about-contact">{AUTHOR_EMAIL}</span>
                <Button size="sm" variant="outline" tone="primary" onClick={onWriteMail}>
                  写邮件
                </Button>
              </span>
            </li>
          </ul>
        </section>
      </div>
    </DialogShell>
  );
}
