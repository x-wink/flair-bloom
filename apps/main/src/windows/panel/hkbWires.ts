import type { KeyId } from './components/KeyCapture';
// 带扩展名：scripts/hkb-wires.test.ts 用 Node 类型剥离直接跑本文件，无扩展名的相对导入会报
// ERR_MODULE_NOT_FOUND；Vite 与 tsc（allowImportingTsExtensions）都认这种写法。
import { keyToken } from './keyToken.ts';

type BurstMode = 'hold' | 'toggle';

/** 走线只关心规则的这几个字段。 */
export interface WireRule {
  id: string;
  enabled: boolean;
  trigger_key: KeyId;
  target_key: KeyId;
  mode: BurstMode;
  stop_key: KeyId | null;
}

/** 一条走线：启动键 → 连发按键（实线）或 启动键 → 停止键（虚线）。 */
export interface Wire {
  ruleId: string;
  from: string;
  to: string;
  kind: 'target' | 'stop';
  mode: BurstMode;
  enabled: boolean;
}

export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** 横版画不出来的「高级」关系：启动键 ≠ 连发按键、切换连发的独立停止键。 */
export function buildWires(rules: WireRule[]): Wire[] {
  const wires: Wire[] = [];
  for (const r of rules) {
    const from = keyToken(r.trigger_key);
    const base = { ruleId: r.id, from, mode: r.mode, enabled: r.enabled };
    const target = keyToken(r.target_key);
    if (target !== from) wires.push({ ...base, to: target, kind: 'target' });
    if (r.mode === 'toggle' && r.stop_key) {
      const stop = keyToken(r.stop_key);
      if (stop !== from) wires.push({ ...base, to: stop, kind: 'stop' });
    }
  }
  return wires;
}

/** 悬停会画线的键：任一走线的端点。 */
export function wiredKeys(wires: Wire[]): Set<string> {
  const keys = new Set<string>();
  for (const w of wires) {
    keys.add(w.from);
    keys.add(w.to);
  }
  return keys;
}

/** 拐角切角长度：45° 斜边让走线读起来像 PCB 排线而不是表格线。 */
const CHAMFER = 6;
/** 并行走线的间距，多条从同一键出发时按它扇出。 */
export const FAN_GAP = 3.5;
/** 同一行两键之间走行上方的通道，离键帽上沿的距离。 */
const LANE_ABOVE = 8;

type Point = [number, number];

/**
 * 曼哈顿路由：竖 → 横 → 竖，每个拐角切 45°。`fan` 是本条在并行组里的偏移（px），
 * 同时作用于起止点的横坐标与水平通道，使并行的几条互不重叠。
 */
export function routeWire(from: Rect, to: Rect, fan: number): Point[] {
  const fx = from.left + from.width / 2 + fan;
  const tx = to.left + to.width / 2 + fan;
  const sameRow = Math.abs(from.top - to.top) < Math.min(from.height, to.height);
  if (sameRow) {
    // 同一行：从上沿出、走行上方的通道、从上沿进。
    const laneY = Math.min(from.top, to.top) - LANE_ABOVE + fan;
    return [
      [fx, from.top],
      [fx, laneY],
      [tx, laneY],
      [tx, to.top],
    ];
  }
  const down = to.top > from.top;
  const sy = down ? from.top + from.height : from.top;
  const ty = down ? to.top : to.top + to.height;
  const laneY = (sy + ty) / 2 + fan;
  return [
    [fx, sy],
    [fx, laneY],
    [tx, laneY],
    [tx, ty],
  ];
}

/** 把折线转成 SVG path，拐角按 CHAMFER 切成 45° 斜边（段太短时切角随之缩小）。 */
export function chamferPath(points: Point[]): string {
  if (points.length === 0) return '';
  const [x0, y0] = points[0];
  let d = `M${fmt(x0)} ${fmt(y0)}`;
  for (let i = 1; i < points.length - 1; i++) {
    const [px, py] = points[i - 1];
    const [cx, cy] = points[i];
    const [nx, ny] = points[i + 1];
    const inLen = Math.hypot(cx - px, cy - py);
    const outLen = Math.hypot(nx - cx, ny - cy);
    const c = Math.min(CHAMFER, inLen / 2, outLen / 2);
    if (c <= 0) continue;
    const ax = cx - ((cx - px) / inLen) * c;
    const ay = cy - ((cy - py) / inLen) * c;
    const bx = cx + ((nx - cx) / outLen) * c;
    const by = cy + ((ny - cy) / outLen) * c;
    d += ` L${fmt(ax)} ${fmt(ay)} L${fmt(bx)} ${fmt(by)}`;
  }
  const [lx, ly] = points[points.length - 1];
  return `${d} L${fmt(lx)} ${fmt(ly)}`;
}

/** 并行组内第 i 条（共 n 条）的扇出偏移，居中对称。 */
export function fanOffset(i: number, n: number): number {
  return (i - (n - 1) / 2) * FAN_GAP;
}

export interface PlacedWire {
  id: string;
  d: string;
  start: Point;
  end: Point;
  dir: 'out' | 'in';
  wire: Wire;
}

/**
 * 悬停 `hoverKey` 时要画的走线：从它出去的（out）与指向它的（in）。`rectOf` 取不到的端点
 * （不可绑定、当前后端不支持的键）不画。并行的几条按另一端横坐标排序后扇出，线不交叉。
 */
export function placeWires(
  wires: Wire[],
  hoverKey: string,
  rectOf: (token: string) => Rect | undefined,
): PlacedWire[] {
  const placed: PlacedWire[] = [];
  const place = (list: Wire[], dir: 'out' | 'in') => {
    const withRects = list
      .map((w) => ({ w, from: rectOf(w.from), to: rectOf(w.to) }))
      .filter((p): p is { w: Wire; from: Rect; to: Rect } => !!p.from && !!p.to)
      .sort((a, b) => (dir === 'out' ? a.to.left - b.to.left : a.from.left - b.from.left));
    withRects.forEach(({ w, from, to }, i) => {
      const points = routeWire(from, to, fanOffset(i, withRects.length));
      placed.push({
        id: `${dir}-${w.ruleId}-${w.kind}`,
        d: chamferPath(points),
        start: points[0],
        end: points[points.length - 1],
        dir,
        wire: w,
      });
    });
  };
  place(
    wires.filter((w) => w.from === hoverKey),
    'out',
  );
  place(
    wires.filter((w) => w.to === hoverKey && w.from !== hoverKey),
    'in',
  );
  return placed;
}

function fmt(v: number): string {
  return Number.isInteger(v) ? String(v) : v.toFixed(1);
}
