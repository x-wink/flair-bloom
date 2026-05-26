import { type ReactNode, type RefObject, useEffect, useRef } from 'react';
import Overlay from './Overlay';
import './ContextMenu.css';

/**
 * 菜单项类型：普通项（默认）或分割线。
 * - active：当前选中态（自动在末尾渲染 ✓，可被 appendIcon 覆盖）
 * - subtitle：次级说明文字，第二行小字
 * - prependIcon/appendIcon：自定义图标插槽
 */
export type ContextMenuItem =
  | {
      type?: 'item';
      label: string;
      subtitle?: string;
      onClick: () => void;
      danger?: boolean;
      active?: boolean;
      disabled?: boolean;
      prependIcon?: ReactNode;
      appendIcon?: ReactNode;
    }
  | { type: 'divider' };

interface Props {
  open: boolean;
  onClose: () => void;
  target: RefObject<HTMLElement>;
  items: ContextMenuItem[];
  location?: 'bottom-left' | 'bottom-right';
}

function isDivider(it: ContextMenuItem): it is { type: 'divider' } {
  return it.type === 'divider';
}

export default function ContextMenu({
  open,
  onClose,
  target,
  items,
  location = 'bottom-right',
}: Props) {
  const listRef = useRef<HTMLDivElement>(null);

  // 打开时把焦点给到首个可点项，便于键盘导航
  useEffect(() => {
    if (!open) return;
    const t = setTimeout(() => {
      const first = listRef.current?.querySelector<HTMLButtonElement>(
        '.ctx-menu-item:not([disabled])',
      );
      first?.focus();
    }, 0);
    return () => clearTimeout(t);
  }, [open]);

  function focusSibling(dir: 1 | -1) {
    const buttons = Array.from(
      listRef.current?.querySelectorAll<HTMLButtonElement>('.ctx-menu-item:not([disabled])') ?? [],
    );
    if (buttons.length === 0) return;
    const cur = buttons.findIndex((b) => b === document.activeElement);
    let next = cur + dir;
    if (next < 0) next = buttons.length - 1;
    else if (next >= buttons.length) next = 0;
    buttons[next]?.focus();
  }

  return (
    <Overlay open={open} onClose={onClose} target={target} location={location} mask={false}>
      <div className="ctx-menu" ref={listRef} role="menu">
        {items.map((item, i) => {
          if (isDivider(item)) {
            return <div key={`d-${i}`} className="ctx-menu-divider" role="separator" />;
          }
          const cls = [
            'ctx-menu-item',
            item.danger && 'ctx-menu-item--danger',
            item.active && 'ctx-menu-item--active',
            item.disabled && 'ctx-menu-item--disabled',
          ]
            .filter(Boolean)
            .join(' ');
          return (
            <button
              key={`${i}-${item.label}`}
              type="button"
              role="menuitem"
              className={cls}
              disabled={item.disabled}
              onKeyDown={(e) => {
                if (e.key === 'ArrowDown') {
                  e.preventDefault();
                  focusSibling(1);
                } else if (e.key === 'ArrowUp') {
                  e.preventDefault();
                  focusSibling(-1);
                }
              }}
              onClick={() => {
                if (item.disabled) return;
                onClose();
                item.onClick();
              }}
            >
              {item.prependIcon && (
                <span className="ctx-menu-icon ctx-menu-icon--prepend">{item.prependIcon}</span>
              )}
              <span className="ctx-menu-text">
                <span className="ctx-menu-label">{item.label}</span>
                {item.subtitle && <span className="ctx-menu-subtitle">{item.subtitle}</span>}
              </span>
              {item.appendIcon ? (
                <span className="ctx-menu-icon ctx-menu-icon--append">{item.appendIcon}</span>
              ) : item.active ? (
                <span className="ctx-menu-check" aria-hidden="true">
                  ✓
                </span>
              ) : null}
            </button>
          );
        })}
      </div>
    </Overlay>
  );
}
