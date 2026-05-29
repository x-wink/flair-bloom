import { type ReactNode, type RefObject, useEffect, useId, useRef, useState } from 'react';
import Overlay from './Overlay';
import './ContextMenu.css';

/**
 * 菜单项类型：普通项（默认）或分割线。
 * - active：当前选中态（自动在末尾渲染 ✓，可被 appendIcon 覆盖）
 * - subtitle：次级说明文字，第二行小字
 * - prependIcon/appendIcon：自定义图标插槽
 * - children：传入则渲染子菜单（hover/点击展开），此时 onClick 可省略
 * - disabled：禁用项，不响应点击与 hover 展开
 */
export type ContextMenuItem =
  | {
      type?: 'item';
      label: string;
      subtitle?: string;
      onClick?: () => void;
      danger?: boolean;
      active?: boolean;
      disabled?: boolean;
      prependIcon?: ReactNode;
      appendIcon?: ReactNode;
      children?: ContextMenuItem[];
    }
  | { type: 'divider' };

interface Props {
  open: boolean;
  onClose: () => void;
  target: RefObject<HTMLElement | null>;
  items: ContextMenuItem[];
  location?: 'bottom-left' | 'bottom-right' | 'right-start' | 'left-start';
  /** 内部使用：通知最外层关闭整条菜单链 */
  onCloseChain?: () => void;
  /** 内部使用：菜单链共享的 chainId，根菜单用它做 outside-click 判定 */
  chainId?: string;
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
  onCloseChain,
  chainId,
}: Props) {
  const listRef = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const [openSubIndex, setOpenSubIndex] = useState<number | null>(null);
  const closeChain = onCloseChain ?? onClose;
  const isRoot = !onCloseChain;
  const generatedId = useId();
  const cid = chainId ?? generatedId;

  // 打开时把焦点给到首个可点项，便于键盘导航
  useEffect(() => {
    if (!open) {
      setOpenSubIndex(null);
      return;
    }
    const t = setTimeout(() => {
      const first = listRef.current?.querySelector<HTMLButtonElement>(
        '.ctx-menu-item:not([disabled])',
      );
      first?.focus();
    }, 0);
    return () => clearTimeout(t);
  }, [open]);

  // 根菜单接管 outside-click 与 Esc：覆盖整条菜单链（避免点子菜单误关父菜单）
  useEffect(() => {
    if (!open || !isRoot) return;
    function onDown(e: MouseEvent) {
      const t = e.target as Element | null;
      if (!t) return;
      if (t.closest(`[data-ctx-chain="${cid}"]`)) return;
      if (target.current?.contains(t)) return;
      onClose();
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('mousedown', onDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [open, isRoot, cid, onClose, target]);

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
    <Overlay
      open={open}
      onClose={onClose}
      target={target}
      location={location}
      mask={false}
      closeOnBackdrop={false}
    >
      <div className="ctx-menu" ref={listRef} role="menu" data-ctx-chain={cid}>
        {items.map((item, i) => {
          if (isDivider(item)) {
            return <div key={`d-${i}`} className="ctx-menu-divider" role="separator" />;
          }
          const hasChildren = !!item.children?.length;
          const isSubOpen = openSubIndex === i;
          const cls = [
            'ctx-menu-item',
            item.danger && 'ctx-menu-item--danger',
            item.active && 'ctx-menu-item--active',
            item.disabled && 'ctx-menu-item--disabled',
            hasChildren && 'ctx-menu-item--has-children',
            isSubOpen && 'ctx-menu-item--sub-open',
          ]
            .filter(Boolean)
            .join(' ');
          return (
            <div key={`${i}-${item.label}`} className="ctx-menu-row">
              <button
                ref={(el) => {
                  itemRefs.current[i] = el;
                }}
                type="button"
                role="menuitem"
                className={cls}
                disabled={item.disabled}
                aria-haspopup={hasChildren ? 'menu' : undefined}
                aria-expanded={hasChildren ? isSubOpen : undefined}
                onMouseEnter={() => {
                  if (item.disabled) return;
                  setOpenSubIndex(hasChildren ? i : null);
                }}
                onKeyDown={(e) => {
                  if (e.key === 'ArrowDown') {
                    e.preventDefault();
                    focusSibling(1);
                  } else if (e.key === 'ArrowUp') {
                    e.preventDefault();
                    focusSibling(-1);
                  } else if (e.key === 'ArrowRight' && hasChildren) {
                    e.preventDefault();
                    setOpenSubIndex(i);
                  } else if (e.key === 'ArrowLeft' && !isRoot) {
                    e.preventDefault();
                    onClose();
                  }
                }}
                onClick={() => {
                  if (item.disabled) return;
                  if (hasChildren) {
                    setOpenSubIndex(isSubOpen ? null : i);
                    return;
                  }
                  closeChain();
                  item.onClick?.();
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
                ) : hasChildren ? (
                  <span className="ctx-menu-arrow" aria-hidden="true">
                    ›
                  </span>
                ) : item.active ? (
                  <span className="ctx-menu-check" aria-hidden="true">
                    ✓
                  </span>
                ) : null}
              </button>
              {hasChildren && (
                <ContextMenu
                  open={isSubOpen}
                  onClose={() => setOpenSubIndex((v) => (v === i ? null : v))}
                  target={{ current: itemRefs.current[i] } as RefObject<HTMLElement | null>}
                  items={item.children!}
                  location="right-start"
                  onCloseChain={closeChain}
                  chainId={cid}
                />
              )}
            </div>
          );
        })}
      </div>
    </Overlay>
  );
}
