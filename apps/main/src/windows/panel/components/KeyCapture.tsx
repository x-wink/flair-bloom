import { useState } from 'react';

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
  Space: 0x20,
  Enter: 0x0d,
  Escape: 0x1b,
  Backspace: 0x08,
  Tab: 0x09,
  ArrowUp: 0x26,
  ArrowDown: 0x28,
  ArrowLeft: 0x25,
  ArrowRight: 0x27,
};

export function keyLabel(vk: number): string {
  return KEY_NAMES[vk] ?? (vk ? `0x${vk.toString(16).toUpperCase()}` : '—');
}

interface Props {
  value: number;
  onChange: (vk: number) => void;
}

export default function KeyCapture({ value, onChange }: Props) {
  const [capturing, setCapturing] = useState(false);

  function handleKeyDown(e: React.KeyboardEvent) {
    e.preventDefault();
    const vk = BROWSER_VK[e.code];
    if (vk) {
      onChange(vk);
      setCapturing(false);
    }
  }

  return (
    <button
      className={`key-capture${capturing ? ' capturing' : ''}`}
      onKeyDown={capturing ? handleKeyDown : undefined}
      onClick={() => setCapturing(true)}
      onBlur={() => setCapturing(false)}
    >
      {capturing ? '按下按键…' : keyLabel(value)}
    </button>
  );
}
