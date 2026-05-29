import { useEffect, useState } from 'react';
import './KeyCapture.css';

/** 鼠标按钮枚举（与 Rust 端 `MouseButton` 共享 wire format）。 */
export type MouseButton = 'left' | 'right' | 'middle' | 'x1' | 'x2' | 'wheel_up' | 'wheel_down';

/**
 * 按键标识：键盘 VK 或鼠标按钮。
 * JSON 形态：`{kind:"keyboard",code:81}` / `{kind:"mouse",code:"left"}`。
 */
export type KeyId = { kind: 'keyboard'; code: number } | { kind: 'mouse'; code: MouseButton };

export const keyboardKey = (vk: number): KeyId => ({ kind: 'keyboard', code: vk });
export const mouseKey = (btn: MouseButton): KeyId => ({ kind: 'mouse', code: btn });

const KEY_NAMES: Record<number, string> = {
  0x41: 'A',
  0x42: 'B',
  0x43: 'C',
  0x44: 'D',
  0x45: 'E',
  0x46: 'F',
  0x47: 'G',
  0x48: 'H',
  0x49: 'I',
  0x4a: 'J',
  0x4b: 'K',
  0x4c: 'L',
  0x4d: 'M',
  0x4e: 'N',
  0x4f: 'O',
  0x50: 'P',
  0x51: 'Q',
  0x52: 'R',
  0x53: 'S',
  0x54: 'T',
  0x55: 'U',
  0x56: 'V',
  0x57: 'W',
  0x58: 'X',
  0x59: 'Y',
  0x5a: 'Z',
  0x30: '0',
  0x31: '1',
  0x32: '2',
  0x33: '3',
  0x34: '4',
  0x35: '5',
  0x36: '6',
  0x37: '7',
  0x38: '8',
  0x39: '9',
  0x70: 'F1',
  0x71: 'F2',
  0x72: 'F3',
  0x73: 'F4',
  0x74: 'F5',
  0x75: 'F6',
  0x76: 'F7',
  0x77: 'F8',
  0x78: 'F9',
  0x79: 'F10',
  0x7a: 'F11',
  0x7b: 'F12',
  0x7c: 'F13',
  0x7d: 'F14',
  0x7e: 'F15',
  0x7f: 'F16',
  0x80: 'F17',
  0x81: 'F18',
  0x82: 'F19',
  0x83: 'F20',
  0x84: 'F21',
  0x85: 'F22',
  0x86: 'F23',
  0x87: 'F24',
  0x60: '小键盘 0',
  0x61: '小键盘 1',
  0x62: '小键盘 2',
  0x63: '小键盘 3',
  0x64: '小键盘 4',
  0x65: '小键盘 5',
  0x66: '小键盘 6',
  0x67: '小键盘 7',
  0x68: '小键盘 8',
  0x69: '小键盘 9',
  0x6a: '小键盘 *',
  0x6b: '小键盘 +',
  0x6d: '小键盘 -',
  0x6e: '小键盘 .',
  0x6f: '小键盘 /',
  0xba: ';',
  0xbb: '=',
  0xbc: ',',
  0xbd: '-',
  0xbe: '.',
  0xbf: '/',
  0xc0: '`',
  0xdb: '[',
  0xdc: '\\',
  0xdd: ']',
  0xde: "'",
  0x14: 'CapsLock',
  0x5d: '菜单键',
  0x90: 'NumLock',
  0x91: 'ScrollLock',
  0x13: 'Pause',
  0x2d: 'Insert',
  0x2e: 'Delete',
  0x24: 'Home',
  0x23: 'End',
  0x21: 'PageUp',
  0x22: 'PageDown',
  0x20: 'Space',
  0x0d: 'Enter',
  0x1b: 'Esc',
  0x08: 'Backspace',
  0x09: 'Tab',
  0x26: '↑',
  0x28: '↓',
  0x25: '←',
  0x27: '→',
};

export const BROWSER_VK: Record<string, number> = {
  KeyA: 0x41,
  KeyB: 0x42,
  KeyC: 0x43,
  KeyD: 0x44,
  KeyE: 0x45,
  KeyF: 0x46,
  KeyG: 0x47,
  KeyH: 0x48,
  KeyI: 0x49,
  KeyJ: 0x4a,
  KeyK: 0x4b,
  KeyL: 0x4c,
  KeyM: 0x4d,
  KeyN: 0x4e,
  KeyO: 0x4f,
  KeyP: 0x50,
  KeyQ: 0x51,
  KeyR: 0x52,
  KeyS: 0x53,
  KeyT: 0x54,
  KeyU: 0x55,
  KeyV: 0x56,
  KeyW: 0x57,
  KeyX: 0x58,
  KeyY: 0x59,
  KeyZ: 0x5a,
  Digit0: 0x30,
  Digit1: 0x31,
  Digit2: 0x32,
  Digit3: 0x33,
  Digit4: 0x34,
  Digit5: 0x35,
  Digit6: 0x36,
  Digit7: 0x37,
  Digit8: 0x38,
  Digit9: 0x39,
  F1: 0x70,
  F2: 0x71,
  F3: 0x72,
  F4: 0x73,
  F5: 0x74,
  F6: 0x75,
  F7: 0x76,
  F8: 0x77,
  F9: 0x78,
  F10: 0x79,
  F11: 0x7a,
  F12: 0x7b,
  F13: 0x7c,
  F14: 0x7d,
  F15: 0x7e,
  F16: 0x7f,
  F17: 0x80,
  F18: 0x81,
  F19: 0x82,
  F20: 0x83,
  F21: 0x84,
  F22: 0x85,
  F23: 0x86,
  F24: 0x87,
  Numpad0: 0x60,
  Numpad1: 0x61,
  Numpad2: 0x62,
  Numpad3: 0x63,
  Numpad4: 0x64,
  Numpad5: 0x65,
  Numpad6: 0x66,
  Numpad7: 0x67,
  Numpad8: 0x68,
  Numpad9: 0x69,
  NumpadMultiply: 0x6a,
  NumpadAdd: 0x6b,
  NumpadSubtract: 0x6d,
  NumpadDecimal: 0x6e,
  NumpadDivide: 0x6f,
  NumpadEnter: 0x0d,
  Semicolon: 0xba,
  Equal: 0xbb,
  Comma: 0xbc,
  Minus: 0xbd,
  Period: 0xbe,
  Slash: 0xbf,
  Backquote: 0xc0,
  BracketLeft: 0xdb,
  Backslash: 0xdc,
  BracketRight: 0xdd,
  Quote: 0xde,
  CapsLock: 0x14,
  ContextMenu: 0x5d,
  NumLock: 0x90,
  ScrollLock: 0x91,
  Pause: 0x13,
  Insert: 0x2d,
  Delete: 0x2e,
  Home: 0x24,
  End: 0x23,
  PageUp: 0x21,
  PageDown: 0x22,
  Space: 0x20,
  Enter: 0x0d,
  Escape: 0x1b,
  Backspace: 0x08,
  Tab: 0x09,
  ArrowUp: 0x26,
  ArrowDown: 0x28,
  ArrowLeft: 0x25,
  ArrowRight: 0x27,
} as const;

const MOUSE_NAMES: Record<MouseButton, string> = {
  left: '鼠标左键',
  right: '鼠标右键',
  middle: '鼠标中键',
  x1: '侧键 1',
  x2: '侧键 2',
  wheel_up: '滚轮↑',
  wheel_down: '滚轮↓',
};

export function keyLabel(key: KeyId | null | undefined): string {
  if (!key) return '—';
  if (key.kind === 'mouse') return MOUSE_NAMES[key.code];
  const vk = key.code;
  if (vk === 0) return '—';
  return KEY_NAMES[vk] ?? `0x${vk.toString(16).toUpperCase()}`;
}

interface Props {
  value: KeyId | null;
  onChange: (key: KeyId | null) => void;
  /** 为 true 时，允许右键清空已绑定按键。 */
  nullable?: boolean;
  placeholder?: string;
  /** 冲突级别，用于着色提示。 */
  conflict?: 'error' | 'warning' | null;
}

const MOUSE_BUTTON_MAP: Record<number, MouseButton> = {
  0: 'left',
  1: 'middle',
  2: 'right',
  3: 'x1',
  4: 'x2',
};

export default function KeyCapture({ value, onChange, nullable, placeholder, conflict }: Props) {
  const [capturing, setCapturing] = useState(false);

  useEffect(() => {
    if (!capturing) return;
    const timer = setTimeout(() => setCapturing(false), 5000);
    return () => clearTimeout(timer);
  }, [capturing]);

  // 捕获模式下统一拦截键盘、鼠标和滚轮事件。
  // capture 阶段注册确保优先于其他 handler；preventDefault 阻止右键菜单等默认行为。
  useEffect(() => {
    if (!capturing) return;

    const keyboardHandler = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const vk = BROWSER_VK[e.code];
      if (vk !== undefined) {
        onChange(keyboardKey(vk));
        setCapturing(false);
      }
    };

    const mouseHandler = (e: MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const btn = MOUSE_BUTTON_MAP[e.button];
      if (btn !== undefined) {
        onChange(mouseKey(btn));
        setCapturing(false);
      }
    };

    const wheelHandler = (e: WheelEvent) => {
      e.preventDefault();
      e.stopPropagation();
      onChange(mouseKey(e.deltaY < 0 ? 'wheel_up' : 'wheel_down'));
      setCapturing(false);
    };

    // 阻止右键菜单弹出（React 批处理保证 contextmenu 触发时 capturing 仍为 true）
    const contextMenuHandler = (e: Event) => e.preventDefault();

    window.addEventListener('keydown', keyboardHandler, { capture: true });
    window.addEventListener('mousedown', mouseHandler, { capture: true });
    window.addEventListener('wheel', wheelHandler, { capture: true, passive: false });
    window.addEventListener('contextmenu', contextMenuHandler, { capture: true });
    return () => {
      window.removeEventListener('keydown', keyboardHandler, { capture: true });
      window.removeEventListener('mousedown', mouseHandler, { capture: true });
      window.removeEventListener('wheel', wheelHandler, { capture: true, passive: false });
      window.removeEventListener('contextmenu', contextMenuHandler, { capture: true });
    };
  }, [capturing, onChange]);

  return (
    <button
      className={`key-capture${capturing ? ' capturing' : ''}${!value ? ' key-capture-empty' : ''}${conflict === 'error' ? ' key-capture-error' : conflict === 'warning' ? ' key-capture-warn' : ''}`}
      title={nullable && value ? '左键重新绑定 · 右键清除' : undefined}
      // 右键清除绑定（非捕获状态）
      onContextMenu={(e) => {
        e.preventDefault();
        if (!capturing && nullable && value) {
          onChange(null);
        }
      }}
      onClick={() => setCapturing(true)}
      onBlur={() => setCapturing(false)}
    >
      {capturing ? '按下按键…' : value ? keyLabel(value) : (placeholder ?? '—')}
    </button>
  );
}
