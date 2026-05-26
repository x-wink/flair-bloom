import {
  type CSSProperties,
  type ReactNode,
  createContext,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { createPortal } from 'react-dom';
import './Overlay.css';

// ---------- types ----------

export type Location =
  | 'top'
  | 'bottom'
  | 'left'
  | 'right'
  | 'top-left'
  | 'top-right'
  | 'bottom-left'
  | 'bottom-right'
  | 'right-start'
  | 'right-end'
  | 'left-start'
  | 'left-end';

export type AttachTarget = HTMLElement | 'body' | 'parent';

/** 定位锚点：可传 DOM 元素或 RefObject */
export type TargetRef = HTMLElement | null | React.RefObject<HTMLElement>;

function resolveTarget(t: TargetRef | undefined): HTMLElement | null {
  if (!t) return null;
  if ('current' in t) return t.current;
  return t;
}

export interface OverlayConfig {
  id?: string;
  content: ReactNode;
  attach?: AttachTarget;
  target?: TargetRef;
  location?: Location;
  offset?: number | [number, number];
  mask?: boolean;
  closeOnBackdrop?: boolean;
  onClose?: () => void;
  duration?: number;
}

export interface OverlayInstance {
  id: string;
  close: () => void;
}

export interface OverlayApi {
  open(config: OverlayConfig): OverlayInstance;
  close(id: string): void;
  closeAll(): void;
  update(id: string, patch: Partial<OverlayConfig>): void;
}

interface OverlayItem {
  id: string;
  version: number;
  content: ReactNode;
  attach?: AttachTarget;
  target?: TargetRef;
  location?: Location;
  offset?: number | [number, number];
  mask?: boolean;
  closeOnBackdrop?: boolean;
  onClose?: () => void;
}

// ---------- context ----------

const OverlayContext = createContext<OverlayApi | null>(null);

let _nextId = 0;

// ---------- OverlayRoot ----------

export function OverlayRoot({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<OverlayItem[]>([]);
  const itemsRef = useRef(items);
  itemsRef.current = items;
  const closingRef = useRef(new Set<string>());
  const timersRef = useRef(new Map<string, ReturnType<typeof setTimeout>>());

  const api: OverlayApi = useMemo(() => {
    function close(id: string) {
      if (closingRef.current.has(id)) return;
      closingRef.current.add(id);
      const timer = timersRef.current.get(id);
      if (timer) {
        clearTimeout(timer);
        timersRef.current.delete(id);
      }
      const found = itemsRef.current.find((i) => i.id === id);
      setItems((prev) => prev.filter((i) => i.id !== id));
      found?.onClose?.();
      closingRef.current.delete(id);
    }

    return {
      open(config) {
        const id = config.id ?? `ovl-${++_nextId}`;
        const item: OverlayItem = {
          id,
          version: 0,
          content: config.content,
          attach: config.attach,
          target: config.target,
          location: config.location,
          offset: config.offset,
          mask: config.mask,
          closeOnBackdrop: config.closeOnBackdrop,
          onClose: config.onClose,
        };
        setItems((prev) => [...prev, item]);
        const inst: OverlayInstance = { id, close: () => close(id) };
        if (config.duration && config.duration > 0) {
          timersRef.current.set(
            id,
            setTimeout(() => close(id), config.duration),
          );
        }
        return inst;
      },
      close,
      closeAll() {
        const snapshot = itemsRef.current;
        setItems([]);
        for (const item of snapshot) {
          item.onClose?.();
        }
      },
      update(id, patch) {
        setItems((prev) =>
          prev.map((i) => (i.id === id ? { ...i, ...patch, id, version: i.version + 1 } : i)),
        );
        if (patch.duration !== undefined) {
          const oldTimer = timersRef.current.get(id);
          if (oldTimer) clearTimeout(oldTimer);
          if (patch.duration > 0) {
            timersRef.current.set(
              id,
              setTimeout(() => close(id), patch.duration),
            );
          } else {
            timersRef.current.delete(id);
          }
        }
      },
    };
  }, []);

  // 根级 Escape：只关闭最顶层允许背景关闭的 managed overlay
  useEffect(() => {
    function handleKey(e: KeyboardEvent) {
      if (e.key !== 'Escape') return;
      const items = itemsRef.current;
      for (let i = items.length - 1; i >= 0; i--) {
        const { closeOnBackdrop, onClose } = items[i];
        if (closeOnBackdrop !== false && onClose) {
          onClose();
          return;
        }
      }
    }
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, []);

  return (
    <OverlayContext.Provider value={api}>
      {children}
      {items.map((item) => (
        <Overlay
          key={item.id}
          open
          managed
          version={item.version}
          attach={item.attach}
          target={item.target}
          location={item.location}
          offset={item.offset}
          mask={item.mask}
          closeOnBackdrop={item.closeOnBackdrop}
          onClose={() => api.close(item.id)}
        >
          {item.content}
        </Overlay>
      ))}
    </OverlayContext.Provider>
  );
}

// ---------- useOverlay ----------

export function useOverlay(): OverlayApi {
  const ctx = useContext(OverlayContext);
  if (!ctx) throw new Error('useOverlay must be used inside <OverlayRoot>');
  return ctx;
}

// ---------- helpers ----------

function off(offset: undefined | number | [number, number]): [number, number] {
  if (offset == null) return [0, 0];
  return typeof offset === 'number' ? [offset, offset] : offset;
}

function toPortal(attach: AttachTarget | undefined): HTMLElement | 'parent' {
  if (attach === 'parent') return 'parent';
  if (attach === 'body') return document.body;
  if (attach instanceof HTMLElement) return attach;
  return document.body;
}

const GAP = 6;
const PAD = 12;

function clampPos(p: { left: number; top: number }, size: { w: number; h: number }) {
  return {
    left: Math.max(PAD, Math.min(p.left, window.innerWidth - size.w - PAD)),
    top: Math.max(PAD, Math.min(p.top, window.innerHeight - size.h - PAD)),
  };
}

function computePos(
  targetRect: { left: number; top: number; width: number; height: number } | null,
  loc: Location | undefined,
  offset: [number, number],
  self: { w: number; h: number },
): { left: number; top: number } | null {
  const [ox, oy] = offset;

  if (targetRect) {
    const tl = targetRect.left,
      tt = targetRect.top,
      tw = targetRect.width,
      th = targetRect.height;
    const l = loc ?? 'bottom';
    const map: Record<string, () => { left: number; top: number }> = {
      bottom: () => ({ left: tl + tw / 2 - self.w / 2 + ox, top: tt + th + GAP + oy }),
      top: () => ({ left: tl + tw / 2 - self.w / 2 + ox, top: tt - self.h - GAP + oy }),
      left: () => ({ left: tl - self.w - GAP + ox, top: tt + th / 2 - self.h / 2 + oy }),
      right: () => ({ left: tl + tw + GAP + ox, top: tt + th / 2 - self.h / 2 + oy }),
      'bottom-left': () => ({ left: tl + ox, top: tt + th + GAP + oy }),
      'bottom-right': () => ({ left: tl + tw - self.w + ox, top: tt + th + GAP + oy }),
      'top-left': () => ({ left: tl + ox, top: tt - self.h - GAP + oy }),
      'top-right': () => ({ left: tl + tw - self.w + ox, top: tt - self.h - GAP + oy }),
      'right-start': () => ({ left: tl + tw + GAP + ox, top: tt + oy }),
      'right-end': () => ({ left: tl + tw + GAP + ox, top: tt + th - self.h + oy }),
      'left-start': () => ({ left: tl - self.w - GAP + ox, top: tt + oy }),
      'left-end': () => ({ left: tl - self.w - GAP + ox, top: tt + th - self.h + oy }),
    };
    return clampPos((map[l] ?? map['bottom'])(), self);
  }

  if (!loc) return null;

  const vw = window.innerWidth,
    vh = window.innerHeight;
  const map: Record<string, () => { left: number; top: number }> = {
    top: () => ({ left: (vw - self.w) / 2 + ox, top: PAD + oy }),
    bottom: () => ({ left: (vw - self.w) / 2 + ox, top: vh - self.h - PAD + oy }),
    left: () => ({ left: PAD + ox, top: (vh - self.h) / 2 + oy }),
    right: () => ({ left: vw - self.w - PAD + ox, top: (vh - self.h) / 2 + oy }),
    'top-left': () => ({ left: PAD + ox, top: PAD + oy }),
    'top-right': () => ({ left: vw - self.w - PAD + ox, top: PAD + oy }),
    'bottom-left': () => ({ left: PAD + ox, top: vh - self.h - PAD + oy }),
    'bottom-right': () => ({ left: vw - self.w - PAD + ox, top: vh - self.h - PAD + oy }),
  };
  return clampPos((map[loc] ?? map['top'])(), self);
}

// ---------- declarative Overlay component ----------

interface OverlayProps {
  open: boolean;
  onClose?: () => void;
  children: ReactNode;
  attach?: AttachTarget;
  target?: TargetRef;
  location?: Location;
  offset?: number | [number, number];
  mask?: boolean;
  closeOnBackdrop?: boolean;
  /** 由 OverlayRoot 管理时设为 true，Escape 由根级统一处理 */
  managed?: boolean;
  /** 内容版本号，递增时重新计算锚定位置 */
  version?: number;
}

function Overlay({
  open,
  onClose,
  children,
  attach,
  target,
  location,
  offset,
  mask: maskProp,
  closeOnBackdrop = true,
  managed = false,
  version = 0,
}: OverlayProps) {
  const [anchored, setAnchored] = useState<CSSProperties>({
    position: 'absolute',
    left: 0,
    top: 0,
    zIndex: 99999,
    visibility: 'hidden',
  });
  const childRef = useRef<HTMLDivElement>(null);
  const maskRef = useRef<HTMLDivElement>(null);
  const outsideCleanup = useRef<(() => void) | null>(null);

  const hasMask = maskProp ?? target == null;
  const anchor = !!(target || location);
  const portalTo = toPortal(attach);
  const gap = off(offset);

  useLayoutEffect(() => {
    if (!open) {
      setAnchored({ position: 'absolute', left: 0, top: 0, zIndex: 99999, visibility: 'hidden' });
      return;
    }
    if (!anchor) return;

    // version bump: 仅重算位置，不先隐藏以免闪烁
    if (version === 0) {
      setAnchored({ position: 'absolute', left: 0, top: 0, zIndex: 99999, visibility: 'hidden' });
    }

    const frame = requestAnimationFrame(() => {
      const el = childRef.current;
      if (!el) return;
      const cr = el.getBoundingClientRect();
      const tEl = resolveTarget(target);
      const tr = tEl?.getBoundingClientRect() ?? null;
      const pos = computePos(tr, location, gap, { w: cr.width, h: cr.height });
      if (pos) {
        setAnchored({
          position: 'absolute',
          left: pos.left,
          top: pos.top,
          zIndex: 99999,
          visibility: 'visible',
        });
      } else {
        setAnchored({
          position: 'absolute',
          left: 0,
          top: 0,
          zIndex: 99999,
          visibility: 'visible',
        });
      }
    });
    return () => cancelAnimationFrame(frame);
  }, [open, target, location, offset, version]);

  useEffect(() => {
    if (!open || managed) return;
    function handleKey(e: KeyboardEvent) {
      if (e.key === 'Escape' && closeOnBackdrop && onClose) onClose();
    }
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [open, closeOnBackdrop, onClose, managed]);

  // Click outside to close for maskless overlays
  useEffect(() => {
    if (!open || hasMask || !closeOnBackdrop || !onClose) return;

    const timer = setTimeout(() => {
      function handleMouseDown(e: MouseEvent) {
        const targetEl = resolveTarget(target);
        if (
          childRef.current &&
          !childRef.current.contains(e.target as Node) &&
          (!targetEl || !targetEl.contains(e.target as Node))
        ) {
          onClose?.();
        }
      }
      document.addEventListener('mousedown', handleMouseDown);
      outsideCleanup.current = () => document.removeEventListener('mousedown', handleMouseDown);
    }, 0);

    return () => {
      clearTimeout(timer);
      outsideCleanup.current?.();
      outsideCleanup.current = null;
    };
  }, [open, hasMask, closeOnBackdrop, target, onClose]);

  if (!open) return null;

  const content = (
    <>
      {hasMask && (
        <div
          ref={maskRef}
          className="overlay-mask"
          onClick={(e) => {
            if (e.target === maskRef.current && closeOnBackdrop && onClose) onClose();
          }}
        />
      )}

      {anchor ? (
        <div ref={childRef} className="overlay-anchored" style={anchored}>
          {children}
        </div>
      ) : (
        <div className="overlay-center">
          <div ref={childRef}>{children}</div>
        </div>
      )}
    </>
  );

  if (portalTo === 'parent') return content;
  return createPortal(content, portalTo);
}

export default Overlay;
