import { invoke } from '@tauri-apps/api/core';
import { LogicalSize } from '@tauri-apps/api/dpi';
import { emit } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { currentMonitor, getCurrentWindow } from '@tauri-apps/api/window';
import type { LazyStore } from '@tauri-apps/plugin-store';
import { useCallback, useEffect, useRef, useState } from 'react';
import {
  fitScale,
  LAYOUT_BASE,
  maxScale,
  PANEL_SCALE_EVENT,
  PANEL_SCALE_KEY,
  type PanelLayout,
  sanitizeScales,
  scaledSize,
  type Size,
} from './windowScale.ts';

/** 拖边时每帧都有 resize，停下这么久才写一次 settings 并通知浮窗，不影响界面 */
const PERSIST_DELAY_MS = 400;

async function workAreaMax(base: Size): Promise<number> {
  const monitor = await currentMonitor();
  if (!monitor) return Number.POSITIVE_INFINITY;
  return maxScale(monitor.workArea.size.toLogical(monitor.scaleFactor), base);
}

/**
 * 面板窗口等比缩放：设计尺寸是最小尺寸，拖边时宽高比由后端 `set_panel_aspect` 在 `WM_SIZING`
 * 里锁住，这里只让内容按 webview 缩放跟上。竖版、横版各记一个比例，浮窗跟随当前布局的比例。
 * 用 webview 缩放而不是 CSS zoom：前者等同浏览器缩放，getBoundingClientRect 与鼠标坐标都在
 * 同一套 CSS 像素里，走线与引导气泡的定位不用改。
 */
export function useWindowScale(store: LazyStore) {
  const [scale, setScale] = useState(1);
  const scalesRef = useRef<Record<PanelLayout, number>>({ vertical: 1, horizontal: 1 });
  const layoutRef = useRef<PanelLayout>('vertical');
  const persistTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const loadedRef = useRef<Promise<void> | undefined>(undefined);

  const ensureLoaded = useCallback(() => {
    loadedRef.current ??= store
      .get<unknown>(PANEL_SCALE_KEY)
      .then((v) => {
        scalesRef.current = sanitizeScales(v);
      })
      .catch(() => {});
    return loadedRef.current;
  }, [store]);

  const persist = useCallback(
    (layout: PanelLayout, value: number) => {
      scalesRef.current = { ...scalesRef.current, [layout]: value };
      emit(PANEL_SCALE_EVENT, value).catch(() => {});
      store
        .set(PANEL_SCALE_KEY, scalesRef.current)
        .then(() => store.save())
        .catch(() => {});
    },
    [store],
  );

  /** 按比例定尺寸并居中；内容缩放与持久化交给随之而来的 resize 事件 */
  const apply = useCallback(async (layout: PanelLayout, wanted: number) => {
    const base = LAYOUT_BASE[layout];
    const size = scaledSize(base, Math.min(wanted, await workAreaMax(base)));
    const win = getCurrentWindow();
    await invoke('set_panel_aspect', { width: base.width, height: base.height });
    await win.setMinSize(new LogicalSize(base.width, base.height));
    await win.setSize(new LogicalSize(size.width, size.height));
    await win.center();
  }, []);

  /** 切换布局或启动时按该布局记住的比例定尺寸 */
  const applyLayout = useCallback(
    async (layout: PanelLayout) => {
      await ensureLoaded();
      layoutRef.current = layout;
      try {
        await apply(layout, scalesRef.current[layout]);
      } catch {
        /* 尺寸调整失败不阻断交互 */
      }
    },
    [apply, ensureLoaded],
  );

  const resetScale = useCallback(async () => {
    try {
      await apply(layoutRef.current, 1);
    } catch {
      /* 同上 */
    }
  }, [apply]);

  useEffect(() => {
    const win = getCurrentWindow();
    const unlisten = win.onResized(async ({ payload }) => {
      const size = payload.toLogical(await win.scaleFactor());
      const layout = layoutRef.current;
      const base = LAYOUT_BASE[layout];
      // 最小化时 Windows 报 0×0；有最小尺寸兜着，低于设计尺寸的都不是用户拖出来的
      if (size.width < base.width - 1 || size.height < base.height - 1) return;
      const next = fitScale(size, base, await workAreaMax(base));
      getCurrentWebview()
        .setZoom(next)
        .catch(() => {});
      setScale(next);
      // 切布局时 setMinSize 先把窗口撑到中间尺寸，会算出一个过渡比例；去抖只留最后一次
      clearTimeout(persistTimerRef.current);
      persistTimerRef.current = setTimeout(() => persist(layout, next), PERSIST_DELAY_MS);
    });
    return () => {
      clearTimeout(persistTimerRef.current);
      unlisten.then((fn) => fn()).catch(() => {});
    };
  }, [persist]);

  return { scale, applyLayout, resetScale };
}
