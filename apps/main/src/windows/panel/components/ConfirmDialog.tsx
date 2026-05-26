import { type ReactNode, createContext, useCallback, useContext, useEffect, useRef } from 'react';
import '../dialogs/dialog-base.css';
import Button from './Button';
import { useOverlay } from './Overlay';

export type ConfirmTone = 'default' | 'danger';

export interface ConfirmOptions {
  title: string;
  description?: ReactNode;
  body?: ReactNode;
  confirmText?: string;
  cancelText?: string;
  tone?: ConfirmTone;
}

type ConfirmFn = (opts: ConfirmOptions) => Promise<boolean>;

const ConfirmContext = createContext<ConfirmFn | null>(null);

export function ConfirmProvider({ children }: { children: ReactNode }) {
  const overlay = useOverlay();
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
              <Button variant="outline" tone="neutral" onClick={() => close(false)}>
                {opts.cancelText ?? '取消'}
              </Button>
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
      }),
    [overlay],
  );

  return <ConfirmContext.Provider value={confirm}>{children}</ConfirmContext.Provider>;
}

export function useConfirm(): ConfirmFn {
  const ctx = useContext(ConfirmContext);
  if (!ctx) throw new Error('useConfirm must be used inside ConfirmProvider');
  return ctx;
}
