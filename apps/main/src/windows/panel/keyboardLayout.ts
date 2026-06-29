// 横版布局的全键盘 / 鼠标静态结构数据。
//
// 键盘拆成三块：主键区（main）、编辑/方向簇（nav）、小键盘（numpad），各自是若干行。
// 每个键 `vk` 为 Windows 虚拟键码；`bindable` 为 false 的键（修饰键 / PrintScreen 等）
// 仅作键盘观感、不可点——因为连发引擎对它们的注入语义不清晰，横版只允许绑定 KEY_NAMES 已知键。
// `w` 为键宽单位（标准键 = 1），`spacer` 为占位空隙。
//
// 横版采用「单键模型」：点某个键即创建 trigger==target(==stop) 的规则，故每个物理键正好对应一个槽位。

import type { MouseButton } from './components/KeyCapture';

export interface KeyCap {
  /** Windows 虚拟键码（与 KeyCapture 的 KEY_NAMES / BROWSER_VK 对齐）。 */
  vk?: number;
  /** 键帽显示文字；省略时由组件用 keyLabel(vk) 推导。 */
  label?: string;
  /** 键宽单位，默认 1。 */
  w?: number;
  /** 是否可绑定连发；默认 true（仅当带 vk 时有效）。 */
  bindable?: boolean;
  /** 纯占位空隙，不渲染键帽。 */
  spacer?: boolean;
}

export type KeyRow = KeyCap[];

/** 主键区：每行总宽 15 单位，对齐标准 ANSI 布局。 */
export const MAIN_BLOCK: KeyRow[] = [
  [
    { vk: 0x1b, label: 'Esc' },
    { spacer: true, w: 1 },
    { vk: 0x70, label: 'F1' },
    { vk: 0x71, label: 'F2' },
    { vk: 0x72, label: 'F3' },
    { vk: 0x73, label: 'F4' },
    { spacer: true, w: 0.5 },
    { vk: 0x74, label: 'F5' },
    { vk: 0x75, label: 'F6' },
    { vk: 0x76, label: 'F7' },
    { vk: 0x77, label: 'F8' },
    { spacer: true, w: 0.5 },
    { vk: 0x78, label: 'F9' },
    { vk: 0x79, label: 'F10' },
    { vk: 0x7a, label: 'F11' },
    { vk: 0x7b, label: 'F12' },
  ],
  [
    { vk: 0xc0, label: '`' },
    { vk: 0x31, label: '1' },
    { vk: 0x32, label: '2' },
    { vk: 0x33, label: '3' },
    { vk: 0x34, label: '4' },
    { vk: 0x35, label: '5' },
    { vk: 0x36, label: '6' },
    { vk: 0x37, label: '7' },
    { vk: 0x38, label: '8' },
    { vk: 0x39, label: '9' },
    { vk: 0x30, label: '0' },
    { vk: 0xbd, label: '-' },
    { vk: 0xbb, label: '=' },
    { vk: 0x08, label: 'Back', w: 2 },
  ],
  [
    { vk: 0x09, label: 'Tab', w: 1.5 },
    { vk: 0x51, label: 'Q' },
    { vk: 0x57, label: 'W' },
    { vk: 0x45, label: 'E' },
    { vk: 0x52, label: 'R' },
    { vk: 0x54, label: 'T' },
    { vk: 0x59, label: 'Y' },
    { vk: 0x55, label: 'U' },
    { vk: 0x49, label: 'I' },
    { vk: 0x4f, label: 'O' },
    { vk: 0x50, label: 'P' },
    { vk: 0xdb, label: '[' },
    { vk: 0xdd, label: ']' },
    { vk: 0xdc, label: '\\', w: 1.5 },
  ],
  [
    { vk: 0x14, label: 'Caps', w: 1.75 },
    { vk: 0x41, label: 'A' },
    { vk: 0x53, label: 'S' },
    { vk: 0x44, label: 'D' },
    { vk: 0x46, label: 'F' },
    { vk: 0x47, label: 'G' },
    { vk: 0x48, label: 'H' },
    { vk: 0x4a, label: 'J' },
    { vk: 0x4b, label: 'K' },
    { vk: 0x4c, label: 'L' },
    { vk: 0xba, label: ';' },
    { vk: 0xde, label: "'" },
    { vk: 0x0d, label: 'Enter', w: 2.25 },
  ],
  [
    { label: 'Shift', w: 2.25, bindable: false },
    { vk: 0x5a, label: 'Z' },
    { vk: 0x58, label: 'X' },
    { vk: 0x43, label: 'C' },
    { vk: 0x56, label: 'V' },
    { vk: 0x42, label: 'B' },
    { vk: 0x4e, label: 'N' },
    { vk: 0x4d, label: 'M' },
    { vk: 0xbc, label: ',' },
    { vk: 0xbe, label: '.' },
    { vk: 0xbf, label: '/' },
    { label: 'Shift', w: 2.75, bindable: false },
  ],
  [
    { label: 'Ctrl', w: 1.25, bindable: false },
    { label: 'Win', w: 1.25, bindable: false },
    { label: 'Alt', w: 1.25, bindable: false },
    { vk: 0x20, label: 'Space', w: 6.25 },
    { label: 'Alt', w: 1.25, bindable: false },
    { label: 'Win', w: 1.25, bindable: false },
    { vk: 0x5d, label: 'Menu', w: 1.25 },
    { label: 'Ctrl', w: 1.25, bindable: false },
  ],
];

/** 编辑 / 方向簇：每行总宽 3 单位。 */
export const NAV_BLOCK: KeyRow[] = [
  [
    { label: 'PrtSc', bindable: false },
    { vk: 0x91, label: 'ScrLk' },
    { vk: 0x13, label: 'Pause' },
  ],
  [
    { vk: 0x2d, label: 'Ins' },
    { vk: 0x24, label: 'Home' },
    { vk: 0x21, label: 'PgUp' },
  ],
  [
    { vk: 0x2e, label: 'Del' },
    { vk: 0x23, label: 'End' },
    { vk: 0x22, label: 'PgDn' },
  ],
  [{ spacer: true, w: 3 }],
  [
    { spacer: true, w: 1 },
    { vk: 0x26, label: '↑' },
    { spacer: true, w: 1 },
  ],
  [
    { vk: 0x25, label: '←' },
    { vk: 0x28, label: '↓' },
    { vk: 0x27, label: '→' },
  ],
];

/** 小键盘网格单元：4 列 × 5 行，Enter 跨 3 行、0 跨 2 列，贴合真实小键盘。 */
export interface NumpadCell extends KeyCap {
  col: number;
  row: number;
  colSpan?: number;
  rowSpan?: number;
}

export const NUMPAD_CELLS: NumpadCell[] = [
  { vk: 0x90, label: 'NumLk', col: 1, row: 1 },
  { vk: 0x6f, label: '/', col: 2, row: 1 },
  { vk: 0x6a, label: '*', col: 3, row: 1 },
  { vk: 0x6d, label: '-', col: 4, row: 1 },
  { vk: 0x67, label: '7', col: 1, row: 2 },
  { vk: 0x68, label: '8', col: 2, row: 2 },
  { vk: 0x69, label: '9', col: 3, row: 2 },
  { vk: 0x6b, label: '+', col: 4, row: 2 },
  { vk: 0x64, label: '4', col: 1, row: 3 },
  { vk: 0x65, label: '5', col: 2, row: 3 },
  { vk: 0x66, label: '6', col: 3, row: 3 },
  // 小键盘 Enter 与主键 Enter 同 VK(0x0d)，不可单独绑定，作高键观感跨 3 行。
  { label: 'Enter', col: 4, row: 3, rowSpan: 3, bindable: false },
  { vk: 0x61, label: '1', col: 1, row: 4 },
  { vk: 0x62, label: '2', col: 2, row: 4 },
  { vk: 0x63, label: '3', col: 3, row: 4 },
  { vk: 0x60, label: '0', col: 1, row: 5, colSpan: 2 },
  { vk: 0x6e, label: '.', col: 3, row: 5 },
];

export interface MouseCap {
  code: MouseButton;
  label: string;
}

/**
 * 鼠标按键三行布局，模拟真实鼠标位置：
 * 左/中/右一排；侧键 1 + 滚轮↑ 一排；侧键 2 + 滚轮↓ 一排。
 */
export const MOUSE_ROWS: MouseCap[][] = [
  [
    { code: 'left', label: '左键' },
    { code: 'middle', label: '中键' },
    { code: 'right', label: '右键' },
  ],
  [
    { code: 'x1', label: '侧键1' },
    { code: 'wheel_up', label: '滚轮↑' },
  ],
  [
    { code: 'x2', label: '侧键2' },
    { code: 'wheel_down', label: '滚轮↓' },
  ],
];
