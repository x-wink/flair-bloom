import { emit, listen } from '@tauri-apps/api/event';
import type { LazyStore } from '@tauri-apps/plugin-store';
import { useCallback, useEffect, useRef, useState } from 'react';
import {
  VISUAL_ENHANCEMENT_EVENT,
  VISUAL_ENHANCEMENT_KEY,
  visualEnhancementEnabled,
} from './visualEnhancement';

export function useVisualEnhancement(store: LazyStore) {
  const [enabled, setEnabled] = useState(true);
  const revision = useRef(0);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    const load = async () => {
      const stop = await listen<unknown>(VISUAL_ENHANCEMENT_EVENT, ({ payload }) => {
        if (cancelled) return;
        revision.current += 1;
        setEnabled(visualEnhancementEnabled(payload));
      });
      if (cancelled) {
        stop();
        return;
      }
      unlisten = stop;
      const current = revision.current;
      const stored = await store.get<unknown>(VISUAL_ENHANCEMENT_KEY);
      if (!cancelled && revision.current === current) setEnabled(visualEnhancementEnabled(stored));
    };
    void load().catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [store]);

  const update = useCallback(
    async (next: boolean) => {
      const current = ++revision.current;
      setEnabled(next);
      await store.set(VISUAL_ENHANCEMENT_KEY, next);
      await store.save();
      // 较早的保存响应不能把新选择广播回旧值。
      if (revision.current === current) await emit(VISUAL_ENHANCEMENT_EVENT, next);
    },
    [store],
  );

  return { enabled, update };
}
