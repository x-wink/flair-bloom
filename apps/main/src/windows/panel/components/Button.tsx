import {
  type ButtonHTMLAttributes,
  type ReactNode,
  forwardRef,
  useEffect,
  useRef,
  useState,
} from 'react';
import Kbd from './Kbd';
import './Button.css';

export type ButtonVariant = 'solid' | 'outline' | 'ghost' | 'dashed' | 'link' | 'text';
export type ButtonTone = 'primary' | 'neutral' | 'danger' | 'success' | 'warning';
export type ButtonShape = 'rounded' | 'square' | 'pill' | 'circle';
export type ButtonSize = 'sm' | 'md' | 'lg';

const LOADING_MIN_MS = 300;

export interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'type'> {
  /** 视觉变体：默认 solid（纯色） */
  variant?: ButtonVariant;
  /** 主题色：默认 primary */
  tone?: ButtonTone;
  /** 圆角形状：默认 rounded */
  shape?: ButtonShape;
  /** 尺寸：默认 md */
  size?: ButtonSize;
  /** 加载态，自带 spinner 并屏蔽点击；最少展示 300ms 防止闪烁 */
  loading?: boolean;
  /** 占满父容器宽度 */
  block?: boolean;
  /** 仅图标按钮，padding 等比缩成正方形 */
  iconOnly?: boolean;
  /** 前置图标 */
  prependIcon?: ReactNode;
  /** 后置图标 */
  appendIcon?: ReactNode;
  /** 快捷键提示，渲染为按钮内部的 kbd 标签 */
  kbd?: string;
  /** 原生 type，默认 button 防止意外提交表单 */
  htmlType?: 'button' | 'submit' | 'reset';
}

const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  {
    variant = 'solid',
    tone = 'primary',
    shape = 'rounded',
    size = 'md',
    loading = false,
    block = false,
    iconOnly = false,
    prependIcon,
    appendIcon,
    kbd,
    htmlType = 'button',
    disabled,
    className,
    children,
    onClick,
    ...rest
  },
  ref,
) {
  // 加载态最少展示 LOADING_MIN_MS，避免一闪而过
  const [visualLoading, setVisualLoading] = useState(loading);
  const startedAtRef = useRef<number | null>(loading ? Date.now() : null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (loading) {
      if (timerRef.current) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
      startedAtRef.current = Date.now();
      setVisualLoading(true);
      return;
    }
    const startedAt = startedAtRef.current;
    if (startedAt == null) {
      setVisualLoading(false);
      return;
    }
    const elapsed = Date.now() - startedAt;
    const remaining = LOADING_MIN_MS - elapsed;
    if (remaining <= 0) {
      startedAtRef.current = null;
      setVisualLoading(false);
      return;
    }
    timerRef.current = setTimeout(() => {
      startedAtRef.current = null;
      timerRef.current = null;
      setVisualLoading(false);
    }, remaining);
  }, [loading]);

  useEffect(
    () => () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    },
    [],
  );

  const isLoading = visualLoading;
  const isDisabled = disabled || isLoading;
  const cls = [
    'fb-btn',
    `fb-btn--${variant}`,
    `fb-btn--${tone}`,
    `fb-btn--${shape}`,
    `fb-btn--${size}`,
    block && 'fb-btn--block',
    iconOnly && 'fb-btn--icon-only',
    isLoading && 'fb-btn--loading',
    className,
  ]
    .filter(Boolean)
    .join(' ');

  return (
    <button
      ref={ref}
      type={htmlType}
      className={cls}
      disabled={isDisabled}
      aria-busy={isLoading || undefined}
      onClick={(e) => {
        if (isLoading) {
          e.preventDefault();
          return;
        }
        onClick?.(e);
      }}
      {...rest}
    >
      {isLoading && <span className="fb-btn__spinner" aria-hidden="true" />}
      <span className="fb-btn__content">
        {prependIcon && <span className="fb-btn__icon fb-btn__icon--prepend">{prependIcon}</span>}
        {children != null && children !== '' && <span className="fb-btn__label">{children}</span>}
        {kbd && <Kbd>{kbd}</Kbd>}
        {appendIcon && <span className="fb-btn__icon fb-btn__icon--append">{appendIcon}</span>}
      </span>
    </button>
  );
});

export default Button;
