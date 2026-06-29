import React from 'react';
import ReactDOM from 'react-dom/client';
import { invoke } from '@tauri-apps/api/core';
import FloatApp from './FloatApp';
import '../panel/theme.css';

window.addEventListener('contextmenu', (e) => e.preventDefault());

window.addEventListener('error', (e) => {
  invoke('log_from_frontend', {
    level: 'error',
    message: `[float] ${e.message} at ${e.filename}:${e.lineno}:${e.colno}`,
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
    message: `[float] Unhandled rejection: ${reason}`,
  }).catch(() => {});
});

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <FloatApp />
  </React.StrictMode>,
);
