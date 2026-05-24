import {
  ReactNode,
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from 'react';

export type ToastTone = 'info' | 'success' | 'warning' | 'error';

export interface ToastOptions {
  message: ReactNode;
  tone?: ToastTone;
  duration?: number;
}

interface ToastItem extends Required<Omit<ToastOptions, 'duration'>> {
  id: number;
  duration: number;
}

interface ToastApi {
  show: (opts: ToastOptions) => number;
  info: (msg: ReactNode, duration?: number) => number;
  success: (msg: ReactNode, duration?: number) => number;
  warning: (msg: ReactNode, duration?: number) => number;
  error: (msg: ReactNode, duration?: number) => number;
  dismiss: (id: number) => void;
}

const DEFAULT_DURATION: Record<ToastTone, number> = {
  info: 3000,
  success: 2500,
  warning: 4000,
  error: 5000,
};

const ToastContext = createContext<ToastApi | null>(null);

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const idRef = useRef(0);

  const dismiss = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const show = useCallback<ToastApi['show']>((opts) => {
    const tone = opts.tone ?? 'info';
    const id = ++idRef.current;
    const duration = opts.duration ?? DEFAULT_DURATION[tone];
    setToasts((prev) => [...prev, { id, message: opts.message, tone, duration }]);
    return id;
  }, []);

  const api: ToastApi = {
    show,
    info: (msg, d) => show({ message: msg, tone: 'info', duration: d }),
    success: (msg, d) => show({ message: msg, tone: 'success', duration: d }),
    warning: (msg, d) => show({ message: msg, tone: 'warning', duration: d }),
    error: (msg, d) => show({ message: msg, tone: 'error', duration: d }),
    dismiss,
  };

  return (
    <ToastContext.Provider value={api}>
      {children}
      <div className="toast-stack" role="status" aria-live="polite">
        {toasts.map((t) => (
          <ToastItemView key={t.id} item={t} onDismiss={() => dismiss(t.id)} />
        ))}
      </div>
    </ToastContext.Provider>
  );
}

function ToastItemView({ item, onDismiss }: { item: ToastItem; onDismiss: () => void }) {
  useEffect(() => {
    if (item.duration <= 0) return;
    const timer = window.setTimeout(onDismiss, item.duration);
    return () => window.clearTimeout(timer);
  }, [item.duration, onDismiss]);

  return (
    <div className={`toast toast-${item.tone}`}>
      <span className="toast-icon" aria-hidden="true">
        {item.tone === 'success'
          ? '✓'
          : item.tone === 'error'
            ? '✕'
            : item.tone === 'warning'
              ? '!'
              : 'i'}
      </span>
      <span className="toast-message">{item.message}</span>
      <button className="toast-close" onClick={onDismiss} aria-label="关闭">
        ✕
      </button>
    </div>
  );
}

export function useToast(): ToastApi {
  const ctx = useContext(ToastContext);
  if (!ctx) throw new Error('useToast must be used inside ToastProvider');
  return ctx;
}
