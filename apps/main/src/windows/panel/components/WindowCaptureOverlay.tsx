import { useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import Overlay from './Overlay';
import Kbd from './Kbd';

function cancelCapture() {
  void invoke('cancel_binding_pick').catch(() => {});
}

export default function WindowCaptureOverlay() {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const root = document.getElementById('root');
    const wasInert = root?.inert;
    const focus = document.activeElement;
    if (root) root.inert = true;
    ref.current?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      event.stopImmediatePropagation();
      cancelCapture();
    };
    window.addEventListener('keydown', onKey, true);
    return () => {
      window.removeEventListener('keydown', onKey, true);
      if (root) root.inert = wasInert ?? false;
      if (focus instanceof HTMLElement && focus.isConnected) focus.focus();
    };
  }, []);
  return (
    <Overlay
      open
      mask
      onClose={cancelCapture}
      className="recording-overlay-layer window-capture-layer"
    >
      <div
        ref={ref}
        className="recording-dialog window-capture-dialog"
        role="dialog"
        aria-modal="true"
        aria-label="捕获目标窗口"
        tabIndex={-1}
        onClick={cancelCapture}
      >
        <h2>捕获目标窗口</h2>
        <p>点击面板外要绑定的系统窗口</p>
        <p className="window-capture-hint">
          <Kbd>Esc</Kbd> 或点击此浮层取消捕获
        </p>
      </div>
    </Overlay>
  );
}
