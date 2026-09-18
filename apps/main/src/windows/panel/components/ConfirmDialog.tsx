import {
  type ReactNode,
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from 'react';
import '../dialogs/dialog-base.css';
import Button from './Button';
import { useOverlay } from './Overlay';

export type ConfirmTone = 'default' | 'danger';

export interface ConfirmOptions {
  title: string;
  description?: ReactNode;
  body?: ReactNode;
  confirmText?: string;
  /** 传 null 则只显示确认按钮（纯提示型弹窗） */
  cancelText?: string | null;
  tone?: ConfirmTone;
}

type ConfirmFn = (opts: ConfirmOptions) => Promise<boolean>;

const ConfirmContext = createContext<ConfirmFn | null>(null);

// 单独一个 context 而不是塞进 ConfirmFn：消费「有没有确认框开着」的地方（引导教程要让路）
// 与调用 confirm() 的地方不是同一批，合在一起会让所有调用方都因开关变化重渲染。
const ConfirmOpenContext = createContext(false);

export function ConfirmProvider({ children }: { children: ReactNode }) {
  const overlay = useOverlay();
  const [open, setOpen] = useState(false);
  const resolveRef = useRef<((v: boolean) => void) | null>(null);
  const overlayIdRef = useRef<string | null>(null);

  useEffect(() => {
    return () => {
      if (overlayIdRef.current) overlay.close(overlayIdRef.current);
    };
  }, [overlay]);

  const confirm = useCallback<ConfirmFn>(
    (opts) =>
      new Promise<boolean>((resolve) => {
        resolveRef.current?.(false);
        resolveRef.current = resolve;

        function close(result: boolean) {
          resolve(result);
          setOpen(false);
          resolveRef.current = null;
          const id = overlayIdRef.current;
          overlayIdRef.current = null;
          if (id) {
            overlay.close(id);
          }
        }

        const content = (
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h2>{opts.title}</h2>
            {opts.description && <p className="modal-desc">{opts.description}</p>}
            {opts.body}
            <div className="modal-actions">
              {opts.cancelText !== null && (
                <Button variant="outline" tone="neutral" onClick={() => close(false)}>
                  {opts.cancelText ?? '取消'}
                </Button>
              )}
              <Button
                variant="solid"
                tone={opts.tone === 'danger' ? 'danger' : 'primary'}
                onClick={() => close(true)}
              >
                {opts.confirmText ?? '确定'}
              </Button>
            </div>
          </div>
        );

        if (overlayIdRef.current) {
          overlay.update(overlayIdRef.current, { content, onClose: () => close(false) });
        } else {
          const inst = overlay.open({
            content,
            closeOnBackdrop: true,
            onClose: () => close(false),
          });
          overlayIdRef.current = inst.id;
        }
        setOpen(true);
      }),
    [overlay],
  );

  return (
    <ConfirmContext.Provider value={confirm}>
      <ConfirmOpenContext.Provider value={open}>{children}</ConfirmOpenContext.Provider>
    </ConfirmContext.Provider>
  );
}

/** 当前有没有确认框开着。引导教程据此让路，关掉后回到同一步。 */
export function useConfirmOpen(): boolean {
  return useContext(ConfirmOpenContext);
}

export function useConfirm(): ConfirmFn {
  const ctx = useContext(ConfirmContext);
  if (!ctx) throw new Error('useConfirm must be used inside ConfirmProvider');
  return ctx;
}
