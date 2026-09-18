import type { LazyStore } from '@tauri-apps/plugin-store';
import { useCallback, useEffect, useRef, useState } from 'react';

export interface TourProgress {
  completed: string[];
  /** 首启是否已经处理过（自动跑了教程、或判定为老用户静默置位）。 */
  introShown: boolean;
}

const TOURS_KEY = 'tours';
const EMPTY: TourProgress = { completed: [], introShown: false };

function normalize(raw: unknown): TourProgress {
  const v = (raw ?? {}) as Partial<TourProgress>;
  return {
    completed: Array.isArray(v.completed)
      ? v.completed.filter((x): x is string => typeof x === 'string')
      : [],
    introShown: v.introShown === true,
  };
}

/** settings.json 的 `tours` 键；写失败只 reject，提示交给调用方。 */
export function useTourProgress(store: LazyStore) {
  const [progress, setProgress] = useState<TourProgress>(EMPTY);
  const [loaded, setLoaded] = useState(false);
  const progressRef = useRef(progress);
  progressRef.current = progress;

  useEffect(() => {
    let cancelled = false;
    store
      .get<unknown>(TOURS_KEY)
      .then((raw) => {
        if (!cancelled) setProgress(normalize(raw));
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, [store]);

  const persist = useCallback(
    async (next: TourProgress) => {
      // 同一事件里连写两次（完成教程紧接着置 introShown）时中间没有重渲染，
      // 第二次必须读到第一次的结果，否则基于陈旧快照把前一次覆盖掉。
      progressRef.current = next;
      setProgress(next);
      await store.set(TOURS_KEY, next);
      await store.save();
    },
    [store],
  );

  const markCompleted = useCallback(
    (id: string) => {
      const cur = progressRef.current;
      if (cur.completed.includes(id)) return Promise.resolve();
      return persist({ ...cur, completed: [...cur.completed, id] });
    },
    [persist],
  );

  const markIntroShown = useCallback(() => {
    const cur = progressRef.current;
    if (cur.introShown) return Promise.resolve();
    return persist({ ...cur, introShown: true });
  }, [persist]);

  return {
    loaded,
    completed: progress.completed,
    introShown: progress.introShown,
    markCompleted,
    markIntroShown,
  };
}
