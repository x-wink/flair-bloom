import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { WindowBindingStatus } from './windowBinding';

export function useWindowBinding() {
  const [status, setStatus] = useState<WindowBindingStatus>();
  useEffect(() => {
    let disposed = false;
    const accept = (next: WindowBindingStatus) => {
      if (!disposed)
        setStatus((current) => (!current || next.revision >= current.revision ? next : current));
    };
    const listening = listen<WindowBindingStatus>('window-binding-changed', (event) =>
      accept(event.payload),
    );
    listening
      .then(() => invoke<WindowBindingStatus>('get_window_binding_status'))
      .then(accept)
      .catch(() => {});
    return () => {
      disposed = true;
      void listening.then((unlisten) => unlisten()).catch(() => {});
    };
  }, []);
  return status;
}
