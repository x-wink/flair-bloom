import './Tabs.css';

export interface TabItem<T extends string = string> {
  id: T;
  label: string;
  badge?: string;
}

interface Props<T extends string = string> {
  tabs: TabItem<T>[];
  active: T;
  onChange: (id: T) => void;
  /** 布局方向，默认 horizontal */
  direction?: 'horizontal' | 'vertical';
  /** 对齐方式，默认 start */
  align?: 'start' | 'center' | 'end';
  /** 视觉样式：underline（下边框，默认）| pill（圆角按钮） */
  variant?: 'underline' | 'pill';
  /** 撑满容器宽度，每个 tab 平分空间 */
  grow?: boolean;
  className?: string;
}

export default function Tabs<T extends string>({
  tabs,
  active,
  onChange,
  direction = 'horizontal',
  align = 'start',
  variant = 'underline',
  grow = false,
  className,
}: Props<T>) {
  const cls = [
    'fb-tab-bar',
    `fb-tab-bar--${direction}`,
    `fb-tab-bar--${align}`,
    `fb-tab-bar--${variant}`,
    grow && 'fb-tab-bar--grow',
    className,
  ]
    .filter(Boolean)
    .join(' ');

  return (
    <div className={cls} role="tablist">
      {tabs.map((t) => (
        <button
          key={t.id}
          type="button"
          role="tab"
          aria-selected={active === t.id}
          className={`fb-tab${active === t.id ? ' fb-tab--active' : ''}`}
          onClick={() => onChange(t.id)}
        >
          {t.label}
          {t.badge !== undefined && <span className="fb-tab-badge">{t.badge}</span>}
        </button>
      ))}
    </div>
  );
}
