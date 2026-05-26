import React from 'react';
import ReactDOM from 'react-dom/client';
import { invoke } from '@tauri-apps/api/core';
import { ConfirmProvider } from './components/ConfirmDialog';
import { OverlayRoot } from './components/Overlay';
import PanelApp from './PanelApp';
import { ToastProvider } from './components/Toast';
import './theme.css';
import './PanelApp.css';

window.addEventListener('contextmenu', (e) => e.preventDefault());

window.addEventListener('error', (e) => {
  invoke('log_from_frontend', {
    level: 'error',
    message: `${e.message} at ${e.filename}:${e.lineno}:${e.colno}`,
  }).catch(() => {});
});

window.addEventListener('unhandledrejection', (e) => {
  const reason =
    e.reason instanceof Error
      ? (e.reason.stack ?? e.reason.message)
      : typeof e.reason === 'string'
        ? e.reason
        : JSON.stringify(e.reason);
  invoke('log_from_frontend', {
    level: 'error',
    message: `Unhandled rejection: ${reason}`,
  }).catch(() => {});
});

if (!import.meta.env.DEV) {
  window.addEventListener('keydown', (e) => {
    if (e.key === 'F12') {
      e.preventDefault();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && ['I', 'J', 'C'].includes(e.key.toUpperCase())) {
      e.preventDefault();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key.toUpperCase() === 'U') {
      e.preventDefault();
    }
  });
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <OverlayRoot>
      <ToastProvider>
        <ConfirmProvider>
          <PanelApp />
        </ConfirmProvider>
      </ToastProvider>
    </OverlayRoot>
  </React.StrictMode>,
);
