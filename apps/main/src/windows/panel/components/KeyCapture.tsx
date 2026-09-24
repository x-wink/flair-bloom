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

/** KeyId 相等判断；任一为空视为不等。 */
export function keyEq(a: KeyId | null | undefined, b: KeyId | null | undefined): boolean {
  if (!a || !b) return false;
  return a.kind === b.kind && a.code === b.code;
}

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
  0xa0: '左 Shift',
  0xa1: '右 Shift',
  0xa2: '左 Ctrl',
  0xa3: '右 Ctrl',
  0xa4: '左 Alt',
  0xa5: '右 Alt',
  0x5b: '左 Win',
  0x5c: '右 Win',
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

/**
 * 修饰键的左右独立 VK。与 [`BROWSER_VK`] 分开维护：修饰键只允许绑定全局热键，
 * 不允许做连发规则的触发键 / 目标键——连发 Alt / Win 会持续触发系统菜单语义，
 * 且 `WM_SYSKEY*` 通道下的注入行为在各游戏里不一致。
 */
export const MODIFIER_VK: Record<string, number> = {
  ShiftLeft: 0xa0,
  ShiftRight: 0xa1,
  ControlLeft: 0xa2,
  ControlRight: 0xa3,
  AltLeft: 0xa4,
  AltRight: 0xa5,
  MetaLeft: 0x5b,
  MetaRight: 0x5c,
} as const;

/**
 * 一个录入槽位的允许集，由后端 `get_key_policy` 下发。
 *
 * 前端不再自行维护「哪些键能绑在哪」：能力（当前输入后端注入得了什么）与策略（产品上
 * 让不让绑）都只有 `packages/qzh-profile/src/key_policy.rs` 一份，配置文件导入与托盘
 * 切换配置根本不经过前端，判定必须以后端为准。
 */
export interface SlotPolicy {
  /** 是否接受普通键盘键。为 false 时该槽位在当前后端下不可用，任何键都收不了。 */
  keyboard: boolean;
  /** 是否接受左右修饰键。 */
  modifiers: boolean;
  /** 接受的鼠标按钮；空数组表示该槽位完全不收鼠标与滚轮。 */
  mouse: MouseButton[];
}

/** 五个槽位的允许集。输入模式切换后需要重新拉取。 */
export interface KeyPolicies {
  hotkey: SlotPolicy;
  trigger: SlotPolicy;
  target: SlotPolicy;
  /** 长按连发里启动键与连发按键重合。 */
  trigger_target: SlotPolicy;
  /** 切换连发里启动键与连发按键重合。后端不支持重合态时是空集。 */
  trigger_target_toggle: SlotPolicy;
  /** 当前后端能否支持切换连发的重合态（启动键与连发按键相同）。 */
  coincident_toggle: boolean;
}

/** 后端未就绪时的兜底：只收键盘普通键，最保守。 */
export const RESTRICTIVE_POLICY: SlotPolicy = { keyboard: true, modifiers: false, mouse: [] };

/** 浏览器 `KeyboardEvent.code` → VK；`includeModifiers` 为 true 时额外接受修饰键。 */
export function vkFromCode(code: string, includeModifiers = false): number | undefined {
  return BROWSER_VK[code] ?? (includeModifiers ? MODIFIER_VK[code] : undefined);
}

const MOUSE_NAMES: Record<MouseButton, string> = {
  left: '鼠标左键',
  right: '鼠标右键',
  middle: '鼠标中键',
  x1: '侧键 1',
  x2: '侧键 2',
  wheel_up: '滚轮上',
  wheel_down: '滚轮下',
};

export function keyLabel(key: KeyId | null | undefined): string {
  if (!key) return '—';
  if (key.kind === 'mouse') return MOUSE_NAMES[key.code];
  const vk = key.code;
  if (vk === 0) return '—';
  return KEY_NAMES[vk] ?? `0x${vk.toString(16).toUpperCase()}`;
}

/**
 * 一次按键被捕获逻辑丢弃的原因。组件本身不弹提示，交由使用方决定文案与呈现方式，
 * 避免基础组件耦合 Toast。`code` 为浏览器 `KeyboardEvent.code`，用于告诉用户按了什么。
 */
export type CaptureReject =
  /** 修饰键，且该槽位不收。 */
  | { reason: 'modifier'; code: string }
  /** 前端的 code → VK 表里没有这个键。 */
  | { reason: 'unknown'; code: string }
  /** 该槽位完全不收鼠标与滚轮（全局热键），或这个鼠标按键根本不认识。 */
  | { reason: 'mouse' }
  /** 该槽位在当前输入模式下整个不可用，换任何键都没用。 */
  | { reason: 'slot-disabled' }
  /** 该槽位收鼠标，但当前输入模式注入不了这个按钮（DD-HID 的侧键）。 */
  | { reason: 'mouse-unsupported'; button: MouseButton };

interface Props {
  value: KeyId | null;
  onChange: (key: KeyId | null) => void;
  /** 为 true 时，允许右键清空已绑定按键。 */
  nullable?: boolean;
  /** 该槽位的允许集，来自后端 `get_key_policy`。 */
  policy: SlotPolicy;
  /** 按键被丢弃时回调，供页面提示用户；不传则静默忽略。 */
  onReject?: (info: CaptureReject) => void;
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

export default function KeyCapture({
  value,
  onChange,
  nullable,
  policy,
  onReject,
  placeholder,
  conflict,
}: Props) {
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
      // 槽位整体不可用时连普通键盘键都不收，先报这个原因，否则用户会以为是自己按错了键
      if (!policy.keyboard) {
        onReject?.({ reason: 'slot-disabled' });
        setCapturing(false);
        return;
      }
      const vk = vkFromCode(e.code, policy.modifiers);
      if (vk !== undefined) {
        onChange(keyboardKey(vk));
        setCapturing(false);
        return;
      }
      onReject?.(
        MODIFIER_VK[e.code] !== undefined
          ? { reason: 'modifier', code: e.code }
          : { reason: 'unknown', code: e.code || e.key },
      );
      setCapturing(false);
    };

    const mouseHandler = (e: MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      // 事件一律吞掉、保持捕获态等待下一次输入，只是不一定录入。
      const btn = MOUSE_BUTTON_MAP[e.button];
      if (btn !== undefined && policy.mouse.includes(btn)) {
        onChange(mouseKey(btn));
        setCapturing(false);
        return;
      }
      if (!policy.keyboard) {
        onReject?.({ reason: 'slot-disabled' });
        setCapturing(false);
        return;
      }
      onReject?.(
        policy.mouse.length === 0 || btn === undefined
          ? { reason: 'mouse' }
          : { reason: 'mouse-unsupported', button: btn },
      );
    };

    const wheelHandler = (e: WheelEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const btn: MouseButton = e.deltaY < 0 ? 'wheel_up' : 'wheel_down';
      if (!policy.mouse.includes(btn)) {
        onReject?.(
          !policy.keyboard
            ? { reason: 'slot-disabled' }
            : policy.mouse.length === 0
              ? { reason: 'mouse' }
              : { reason: 'mouse-unsupported', button: btn },
        );
        return;
      }
      onChange(mouseKey(btn));
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
      window.removeEventListener('wheel', wheelHandler, { capture: true });
      window.removeEventListener('contextmenu', contextMenuHandler, { capture: true });
    };
  }, [capturing, onChange, policy, onReject]);

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
