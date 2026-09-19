import type { LazyStore } from '@tauri-apps/plugin-store';
import { useCallback, useEffect, useRef, useState } from 'react';
import { TOUR_INTRO_VERSION } from './types';

export interface TourProgress {
  completed: string[];
  /** 已经自动展示过的教程版本；空串表示从没展示过。与用户协议的版本位同一套语义。 */
  introVersion: string;
}

const TOURS_KEY = 'tours';
/** 版本位出现之前的 `introShown: true`：当年看到的是没有版本号的那一版。 */
const LEGACY_INTRO_VERSION = '0';
const EMPTY: TourProgress = { completed: [], introVersion: '' };

function normalize(raw: unknown): TourProgress {
  const v = (raw ?? {}) as Partial<TourProgress> & { introShown?: unknown };
  return {
    completed: Array.isArray(v.completed)
      ? v.completed.filter((x): x is string => typeof x === 'string')
      : [],
    introVersion:
      typeof v.introVersion === 'string'
        ? v.introVersion
        : v.introShown === true
          ? LEGACY_INTRO_VERSION
          : '',
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
      // 同一事件里连写两次（完成教程紧接着记版本）时中间没有重渲染，
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

  /** 记下「当前版本的首启教程已经自动出现过」，下次启动不再自动弹。 */
  const markIntroSeen = useCallback(() => {
    const cur = progressRef.current;
    if (cur.introVersion === TOUR_INTRO_VERSION) return Promise.resolve();
    return persist({ ...cur, introVersion: TOUR_INTRO_VERSION });
  }, [persist]);

  return {
    loaded,
    completed: progress.completed,
    /** 当前版本的首启教程是否已经自动出现过 */
    introSeen: progress.introVersion === TOUR_INTRO_VERSION,
    markCompleted,
    markIntroSeen,
  };
}
