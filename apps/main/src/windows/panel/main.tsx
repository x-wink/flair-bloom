import React from 'react';
import ReactDOM from 'react-dom/client';
import { ConfirmProvider } from './ConfirmDialog';
import { OverlayRoot } from './Overlay';
import PanelApp from './PanelApp';
import { ToastProvider } from './Toast';
import './PanelApp.css';

window.addEventListener('contextmenu', (e) => e.preventDefault());

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
