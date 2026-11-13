import { useId, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import Button from './Button';
import Overlay from './Overlay';
import DialogShell from '../dialogs/DialogShell';
import { ChevronIcon, CloseIcon } from './icons';
import StatusLogo from './StatusLogo';
import {
  bindingDescription,
  windowIconUrl,
  targetIsForeground,
  filterBindableWindows,
  type BindableWindow,
  type WindowBindingStatus,
  type WindowIcon,
} from '../windowBinding';
import './WindowBindingControl.css';

export function WindowGlyph() {
  return (
    <svg
      width="18"
      height="18"
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.3"
      aria-hidden="true"
    >
      <rect x="2.5" y="3.5" width="15" height="13" rx="2" />
      <path d="M3 7h14M5 5.2h.5m2 0H8" />
    </svg>
  );
}

export function ChainIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      aria-hidden="true"
    >
      <path
        d="m8 12 4-4M7.5 13.5 6 15a3.5 3.5 0 0 1-5-5l4-4a3.5 3.5 0 0 1 5 0m2.5.5L14 5a3.5 3.5 0 0 1 5 5l-4 4a3.5 3.5 0 0 1-5 0"
        transform="translate(0 -1)"
      />
    </svg>
  );
}

export function TargetLogo({
  icon,
  active,
  pulse = false,
}: {
  icon?: WindowIcon;
  active: boolean;
  pulse?: boolean;
}) {
  const src = useMemo(() => windowIconUrl(icon), [icon]);
  return src ? (
    <StatusLogo className="binding-target-logo" src={src} active={active} pulse={pulse} />
  ) : (
    <span className={`binding-target-logo binding-target-fallback${active ? ' is-active' : ''}`}>
      <WindowGlyph />
    </span>
  );
}

export default function WindowBindingControl({
  status,
  enabled,
  pulse,
  onSelect,
  onError,
}: {
  status: WindowBindingStatus | undefined;
  enabled: boolean;
  pulse: boolean;
  onSelect: (window: BindableWindow | undefined) => Promise<void>;
  onError: (message: string) => void;
}) {
  const ref = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [windows, setWindows] = useState<BindableWindow[]>([]);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const revision = useRef(0);
  const [query, setQuery] = useState('');
  const heading = useId();
  if (!status?.supported) return undefined;

  const select = async (window: BindableWindow | undefined) => {
    setBusy(true);
    try {
      await onSelect(window);
      setOpen(false);
    } catch (error) {
      onError(`窗口绑定失败：${error}`);
    } finally {
      setBusy(false);
    }
  };
  const load = async () => {
    const request = ++revision.current;
    setLoading(true);
    try {
      const list = await invoke<BindableWindow[]>('list_bindable_windows');
      if (request === revision.current) setWindows(list);
    } catch (error) {
      onError(`读取窗口失败：${error}`);
    } finally {
      if (request === revision.current) setLoading(false);
    }
  };
  const pick = async () => {
    setBusy(true);
    try {
      const window = await invoke<BindableWindow | null>('pick_binding_window');
      if (window) await onSelect(window);
    } catch (error) {
      onError(`选取窗口失败：${error}`);
    } finally {
      setBusy(false);
    }
  };
  const active = targetIsForeground(status);
  const filtered = filterBindableWindows(windows, query);
  const close = () => {
    setOpen(false);
    requestAnimationFrame(() => ref.current?.focus());
  };
  return (
    <div className="window-binding-control" data-tour="window-binding">
      <Button
        ref={ref}
        className="window-binding-select"
        size="sm"
        variant="outline"
        tone="neutral"
        disabled={busy || status.picking}
        aria-haspopup="dialog"
        aria-expanded={open}
        title={bindingDescription(status, enabled)}
        prependIcon={
          <TargetLogo
            icon={status.target?.icon}
            active={active && !!status.binding}
            pulse={pulse && enabled}
          />
        }
        appendIcon={<ChevronIcon size={10} />}
        onClick={() => {
          setOpen(!open);
          if (!open) {
            setQuery('');
            void load();
          }
        }}
      >
        {status.picking
          ? '请点选窗口…'
          : status.target?.name || status.binding?.exe || '不限制窗口'}
      </Button>
      <Button
        className="window-binding-pick"
        size="sm"
        variant="outline"
        tone="neutral"
        iconOnly
        loading={busy || status.picking}
        title="点选系统窗口（Esc 取消）"
        aria-label="点选系统窗口"
        onClick={() => void pick()}
      >
        <svg
          width="16"
          height="16"
          viewBox="0 0 20 20"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.4"
          aria-hidden="true"
        >
          <rect x="4.5" y="4.5" width="11" height="11" rx="2" />
          <path d="M10 1v6m0 6v6M1 10h6m6 0h6" />
        </svg>
      </Button>
      <Overlay open={open} onClose={close}>
        <div
          onKeyDownCapture={(event) => {
            if (event.key === 'Escape') {
              event.preventDefault();
              event.stopPropagation();
              close();
            }
          }}
        >
          <DialogShell
            className="window-picker-dialog"
            labelId={heading}
            headerContent={
              <>
                <h2 id={heading} className="window-picker-title">
                  选择目标窗口
                </h2>
                <div className="window-picker-toolbar">
                  <input
                    type="search"
                    autoFocus
                    value={query}
                    onChange={(event) => setQuery(event.target.value)}
                    placeholder="过滤标题或程序名"
                    aria-label="过滤窗口标题或程序名"
                  />
                  <Button
                    size="sm"
                    variant="outline"
                    tone="neutral"
                    loading={loading}
                    onClick={() => void load()}
                  >
                    刷新
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    tone="neutral"
                    disabled={busy || !status.binding}
                    title="解除当前配置的窗口绑定"
                    onClick={() => void select(undefined)}
                  >
                    清空
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    tone="neutral"
                    iconOnly
                    aria-label="关闭窗口列表"
                    onClick={close}
                  >
                    <CloseIcon size={14} />
                  </Button>
                </div>
              </>
            }
            footer={
              <span className="window-picker-current">
                当前：{status.target?.name || status.binding?.exe || '不限制窗口'}
              </span>
            }
          >
            <div className="window-picker-list" aria-label="可绑定窗口" aria-busy={loading}>
              {filtered.map((window) => {
                const selected =
                  window.exe?.toLowerCase() === status.binding?.exe.toLowerCase() &&
                  window.class === status.binding?.class;
                return (
                  <button
                    type="button"
                    key={window.id}
                    className={`window-picker-row${selected ? ' is-selected' : ''}`}
                    disabled={!window.exe || busy}
                    aria-pressed={selected}
                    onClick={() => void select(window)}
                  >
                    <TargetLogo icon={window.icon} active />
                    <span className="window-picker-text">
                      <span>{window.title}</span>
                      <small>{window.exe || '无法识别程序，请尝试以管理员身份运行'}</small>
                    </span>
                    {selected && <span aria-label="已绑定">✓</span>}
                  </button>
                );
              })}
              {!filtered.length && (
                <p className="window-picker-empty">
                  {loading
                    ? '正在读取窗口…'
                    : query.trim()
                      ? '没有匹配的窗口'
                      : '没有可选窗口，请先打开目标程序'}
                </p>
              )}
            </div>
          </DialogShell>
        </div>
      </Overlay>
    </div>
  );
}
