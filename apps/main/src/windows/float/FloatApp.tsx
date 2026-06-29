import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow, LogicalSize, PhysicalPosition } from '@tauri-apps/api/window';
import { LazyStore } from '@tauri-apps/plugin-store';
import iconUrl from '../../assets/icon-64.png';
import { keyLabel, type KeyId } from '../panel/components/KeyCapture';
import {
  applyThemeColor,
  applyThemeMode,
  DEFAULT_THEME_COLOR,
  DEFAULT_THEME_MODE,
  type ThemeMode,
  type ThemeSettings,
} from '../panel/theme';
import '../panel/HorizontalLayout.css';
import './FloatApp.css';

const settingsStore = new LazyStore('settings.json');
const THEME_KEY = 'theme';
const FLOAT_POS_KEY = 'float_position';

interface FloatRule {
  id: string;
  trigger_key: KeyId;
  mode: 'hold' | 'toggle';
  enabled: boolean;
  group: string | null;
}

/** 仅取浮窗渲染所需字段（结构兼容后端 BurstRule）。 */
interface BurstRuleLike {
  id: string;
  trigger_key: KeyId;
  mode: 'hold' | 'toggle';
  enabled: boolean;
  group: string | null;
}

function capClass(r: FloatRule, active: boolean): string {
  return [
    'hkb-cap',
    'hkb-cap--key',
    r.enabled ? (r.mode === 'toggle' ? 'is-toggle' : 'is-hold') : 'is-off',
    active && 'is-active',
  ]
    .filter(Boolean)
    .join(' ');
}

export default function FloatApp() {
  const [active, setActive] = useState(false);
  const [globalEnabled, setGlobalEnabled] = useState(false);
  const [rules, setRules] = useState<FloatRule[]>([]);
  const [activeIds, setActiveIds] = useState<string[]>([]);
  const floatRef = useRef<HTMLDivElement>(null);

  // ── 主题：与主面板同源（settings.json 的 theme），并跟随系统配色 ──
  const themeModeRef = useRef<ThemeMode>(DEFAULT_THEME_MODE);
  useEffect(() => {
    settingsStore
      .get<ThemeSettings>(THEME_KEY)
      .then((v) => {
        const color = v?.color ?? DEFAULT_THEME_COLOR;
        const mode = v?.mode ?? DEFAULT_THEME_MODE;
        themeModeRef.current = mode;
        applyThemeColor(color);
        applyThemeMode(mode);
      })
      .catch(() => {});

    const mql = window.matchMedia('(prefers-color-scheme: dark)');
    const onSystemChange = () => {
      if (themeModeRef.current === 'system') applyThemeMode('system');
    };
    mql.addEventListener('change', onSystemChange);
    return () => mql.removeEventListener('change', onSystemChange);
  }, []);

  // ── 全局开关：驱动背景渐变与呼吸动画（与主面板一致）──
  useEffect(() => {
    invoke<boolean>('get_global_enabled')
      .then(setGlobalEnabled)
      .catch(() => {});
    const unlistenG = listen<boolean>('global-enabled-changed', (e) => setGlobalEnabled(e.payload));
    return () => {
      unlistenG.then((fn) => fn()).catch(() => {});
    };
  }, []);

  // ── 位置：启动恢复上次位置，拖动后持久化 ──
  useEffect(() => {
    settingsStore
      .get<{ x: number; y: number }>(FLOAT_POS_KEY)
      .then((pos) => {
        if (pos && Number.isFinite(pos.x) && Number.isFinite(pos.y)) {
          getCurrentWindow()
            .setPosition(new PhysicalPosition(Math.round(pos.x), Math.round(pos.y)))
            .catch(() => {});
        }
      })
      .catch(() => {});

    let unlisten: (() => void) | undefined;
    let saveTimer: ReturnType<typeof setTimeout> | undefined;
    getCurrentWindow()
      .onMoved(({ payload }) => {
        if (saveTimer) clearTimeout(saveTimer);
        saveTimer = setTimeout(() => {
          settingsStore.set(FLOAT_POS_KEY, { x: payload.x, y: payload.y }).catch(() => {});
          settingsStore.save().catch(() => {});
        }, 400);
      })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {});

    return () => {
      if (saveTimer) clearTimeout(saveTimer);
      unlisten?.();
    };
  }, []);

  // ── 窗口宽高自适应内容：内容 hug，窗口随之收缩 ──
  useEffect(() => {
    const el = floatRef.current;
    if (!el) return;
    const apply = () => {
      const rect = el.getBoundingClientRect();
      const w = Math.ceil(rect.width);
      const h = Math.ceil(rect.height);
      if (w > 0 && h > 0) {
        getCurrentWindow()
          .setSize(new LogicalSize(w, h))
          .catch(() => {});
      }
    };
    const ro = new ResizeObserver(apply);
    ro.observe(el);
    apply();
    return () => ro.disconnect();
  }, []);

  // ── 显隐：由 Rust 的 float-active 事件驱动，只在可见时轮询 ──
  useEffect(() => {
    const unlistenP = listen<boolean>('float-active', (e) => setActive(e.payload));
    // 兜底：挂载时主动查一次当前是否可见（防止错过事件）
    getCurrentWindow()
      .isVisible()
      .then(setActive)
      .catch(() => {});
    return () => {
      unlistenP.then((fn) => fn()).catch(() => {});
    };
  }, []);

  const refreshRules = useCallback(async () => {
    try {
      const list = await invoke<BurstRuleLike[]>('get_rules');
      setRules(
        list.map((r) => ({
          id: r.id,
          trigger_key: r.trigger_key,
          mode: r.mode,
          enabled: r.enabled,
          group: r.group,
        })),
      );
    } catch {
      /* 保留旧缓存 */
    }
  }, []);

  // 激活规则轮询：可见时每 150ms 拉一次 id，停用即清空。
  useEffect(() => {
    if (!active) {
      setActiveIds((prev) => (prev.length === 0 ? prev : []));
      return;
    }
    let cancelled = false;
    void refreshRules();

    const poll = async () => {
      let ids: string[];
      try {
        ids = await invoke<string[]>('get_active_rules');
      } catch {
        return;
      }
      if (cancelled) return;
      setActiveIds((prev) => {
        if (prev.length === ids.length && prev.every((id, i) => id === ids[i])) return prev;
        return ids;
      });
    };
    void poll();
    const timer = setInterval(poll, 150);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [active, refreshRules]);

  const onExpand = () => {
    invoke('show_main_panel').catch(() => {});
  };

  const activeSet = new Set(activeIds);
  // 含激活规则的互斥分组（保持规则出现顺序）
  const activeGroups: string[] = [];
  for (const r of rules) {
    if (r.group && activeSet.has(r.id) && !activeGroups.includes(r.group))
      activeGroups.push(r.group);
  }
  // 激活且无分组的规则，单独成键
  const singles = rules.filter((r) => activeSet.has(r.id) && !r.group);

  const renderCap = (r: FloatRule) => {
    const isActive = activeSet.has(r.id);
    const modeName = r.mode === 'toggle' ? '切换连发' : '按压连发';
    return (
      <span
        key={r.id}
        className={capClass(r, isActive)}
        data-tauri-drag-region
        title={`${keyLabel(r.trigger_key)} · ${modeName}${r.enabled ? '' : '（已停用）'}`}
      >
        <span className="hkb-cap-label">{keyLabel(r.trigger_key)}</span>
      </span>
    );
  };

  const hasAny = singles.length > 0 || activeGroups.length > 0;

  return (
    <div
      ref={floatRef}
      className={`fb-float ${globalEnabled ? 'on' : 'off'}`}
      data-tauri-drag-region
    >
      <img className="fb-float-logo" src={iconUrl} alt="" data-tauri-drag-region />

      <div className="fb-float-content" data-tauri-drag-region>
        {!hasAny ? (
          <span className="fb-float-empty" data-tauri-drag-region>
            未连发
          </span>
        ) : (
          <>
            {singles.map(renderCap)}
            {activeGroups.map((g) => (
              <div
                key={g}
                className="fb-float-group"
                data-tauri-drag-region
                title={`互斥分组 · ${g}`}
              >
                <span className="fb-float-group-name">{g}</span>
                <div className="fb-float-group-caps" data-tauri-drag-region>
                  {rules.filter((r) => r.group === g).map(renderCap)}
                </div>
              </div>
            ))}
          </>
        )}
      </div>

      <button type="button" className="fb-float-expand" title="展开主面板" onClick={onExpand}>
        <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
          <path
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M9 3H5a2 2 0 0 0-2 2v4m18 0V5a2 2 0 0 0-2-2h-4M3 15v4a2 2 0 0 0 2 2h4m6 0h4a2 2 0 0 0 2-2v-4"
          />
        </svg>
      </button>
    </div>
  );
}
