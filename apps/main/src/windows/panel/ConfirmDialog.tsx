import { ReactNode, createContext, useCallback, useContext, useState } from 'react';

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

interface PendingState extends ConfirmOptions {
  resolve: (v: boolean) => void;
}

export function ConfirmProvider({ children }: { children: ReactNode }) {
  const [pending, setPending] = useState<PendingState | null>(null);

  const confirm = useCallback<ConfirmFn>(
    (opts) =>
      new Promise((resolve) => {
        setPending((prev) => {
          prev?.resolve(false);
          return { ...opts, resolve };
        });
      }),
    [],
  );

  function close(result: boolean) {
    if (!pending) return;
    pending.resolve(result);
    setPending(null);
  }

  return (
    <ConfirmContext.Provider value={confirm}>
      {children}
      {pending && (
        <div className="modal-mask" onClick={() => close(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h2>{pending.title}</h2>
            {pending.description && <p className="modal-desc">{pending.description}</p>}
            {pending.body}
            <div className="modal-actions">
              <button className="btn-ghost" onClick={() => close(false)}>
                {pending.cancelText ?? '取消'}
              </button>
              <button
                className={pending.tone === 'danger' ? 'btn-danger' : 'btn-primary'}
                onClick={() => close(true)}
              >
                {pending.confirmText ?? '确定'}
              </button>
            </div>
          </div>
        </div>
      )}
    </ConfirmContext.Provider>
  );
}

export function useConfirm(): ConfirmFn {
  const ctx = useContext(ConfirmContext);
  if (!ctx) throw new Error('useConfirm must be used inside ConfirmProvider');
  return ctx;
}
