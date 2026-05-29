import type { ReactNode } from 'react';
import './DialogShell.css';

interface Props {
  /** 标准标题，与 subtitle 搭配 */
  title?: string;
  /** 标准副标题 */
  subtitle?: string;
  /** 自定义 header 内容，传入时忽略 title/subtitle */
  headerContent?: ReactNode;
  /** header 和 body 之间的插槽（如 Tabs） */
  subheader?: ReactNode;
  /** 主体内容（可滚动） */
  children: ReactNode;
  /** 底部操作区内容 */
  footer?: ReactNode;
  /** 底部对齐：end（默认）| center | spread（首项靠左其余靠右） */
  footerAlign?: 'end' | 'center' | 'spread';
  /** 外层卡片附加 className，用于各弹窗覆盖尺寸 */
  className?: string;
  /** aria-labelledby id */
  labelId?: string;
}

export default function DialogShell({
  title,
  subtitle,
  headerContent,
  subheader,
  children,
  footer,
  footerAlign = 'end',
  className,
  labelId,
}: Props) {
  return (
    <div
      className={`fb-dialog${className ? ` ${className}` : ''}`}
      role="dialog"
      aria-modal="true"
      aria-labelledby={labelId}
    >
      <header className="fb-dialog__header">
        {headerContent ?? (
          <>
            {title && (
              <h2 id={labelId} className="fb-dialog__title">
                {title}
              </h2>
            )}
            {subtitle && <p className="fb-dialog__subtitle">{subtitle}</p>}
          </>
        )}
      </header>

      {subheader && <div className="fb-dialog__subheader">{subheader}</div>}

      <div className="fb-dialog__body">{children}</div>

      {footer && (
        <div className={`fb-dialog__footer fb-dialog__footer--${footerAlign}`}>{footer}</div>
      )}
    </div>
  );
}
