import { invoke } from '@tauri-apps/api/core';
import { useEffect, useRef } from 'react';
import { BROWSER_VK, keyboardKey, type KeyId } from './components/KeyCapture';

interface RelayKeyResult {
  accepted_physical: boolean;
  handled: boolean;
}

function keyToken(key: KeyId): string {
  return `${key.kind}:${key.code}`;
}

function isEditableKeyboardTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return Boolean(
    target.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"])'),
  );
}

/**
 * WebView2 聚焦时 WH_KEYBOARD_LL 不触发；将键盘事件中继到后端引擎统一处理
 * （热键、Toggle 触发键、pressed_keys 维护）。主面板与浮窗共用，避免窗口聚焦时热键被吞。
 * bubble 阶段注册：KeyCapture 在 capture 阶段 stopPropagation()，捕获模式下不干扰。
 */
export function useKeyRelay(): void {
  const relayedKeyDownsRef = useRef<Map<string, KeyId>>(new Map());
  const relayDownInFlightRef = useRef<Set<string>>(new Set());
  const relayedKeyReleasedEarlyRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    const releaseRelayedDowns = () => {
      const pending = [...relayedKeyDownsRef.current.values()];
      relayedKeyDownsRef.current.clear();
      relayDownInFlightRef.current.clear();
      relayedKeyReleasedEarlyRef.current.clear();
      for (const key of pending) {
        invoke('relay_key_event', { key, isUp: true }).catch(() => {});
      }
    };
    const downHandler = (e: KeyboardEvent) => {
      const allowDefault = isEditableKeyboardTarget(e.target);
      if (!allowDefault) e.preventDefault();
      const vk = BROWSER_VK[e.code];
      if (vk !== undefined) {
        const key = keyboardKey(vk);
        if (!e.repeat) {
          const token = keyToken(key);
          relayDownInFlightRef.current.add(token);
          invoke<RelayKeyResult>('relay_key_event', { key, isUp: false })
            .then((result) => {
              relayDownInFlightRef.current.delete(token);
              if (result.accepted_physical) {
                if (relayedKeyReleasedEarlyRef.current.delete(token)) {
                  invoke('relay_key_event', { key, isUp: true }).catch(() => {});
                } else {
                  relayedKeyDownsRef.current.set(token, key);
                }
              }
            })
            .catch(() => {
              relayDownInFlightRef.current.delete(token);
              relayedKeyReleasedEarlyRef.current.delete(token);
            });
        }
      }
    };
    const upHandler = (e: KeyboardEvent) => {
      if (!isEditableKeyboardTarget(e.target)) e.preventDefault();
      const vk = BROWSER_VK[e.code];
      if (vk !== undefined) {
        const key = keyboardKey(vk);
        const token = keyToken(key);
        if (!relayedKeyDownsRef.current.delete(token)) {
          if (relayDownInFlightRef.current.has(token)) {
            relayedKeyReleasedEarlyRef.current.add(token);
          }
          return;
        }
        invoke('relay_key_event', { key, isUp: true }).catch(() => {});
      }
    };
    const visibilityHandler = () => {
      if (document.visibilityState === 'hidden') {
        releaseRelayedDowns();
      }
    };
    window.addEventListener('keydown', downHandler);
    window.addEventListener('keyup', upHandler);
    window.addEventListener('blur', releaseRelayedDowns);
    document.addEventListener('visibilitychange', visibilityHandler);
    return () => {
      window.removeEventListener('keydown', downHandler);
      window.removeEventListener('keyup', upHandler);
      window.removeEventListener('blur', releaseRelayedDowns);
      document.removeEventListener('visibilitychange', visibilityHandler);
      releaseRelayedDowns();
    };
  }, []);
}
