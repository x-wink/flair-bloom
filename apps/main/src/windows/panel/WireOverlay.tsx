import { placeWires, type Rect, type Wire } from './hkbWires';

interface Props {
  hoverKey: string;
  wires: Wire[];
  container: HTMLElement | null;
  caps: Map<string, HTMLElement>;
}

function relRect(el: HTMLElement, origin: DOMRect, container: HTMLElement): Rect {
  const r = el.getBoundingClientRect();
  return {
    left: r.left - origin.left + container.scrollLeft,
    top: r.top - origin.top + container.scrollTop,
    width: r.width,
    height: r.height,
  };
}

/**
 * 悬停键的走线覆盖层：从它出去的用主色、指向它的用成功色；启动 → 连发实线、启动 → 停止虚线；
 * 终点焊盘长按实心圆 / 切换空心圆 / 停止小方块。端点不在图上（不可绑定、当前后端不支持）不画。
 */
export default function WireOverlay({ hoverKey, wires, container, caps }: Props) {
  if (!container) return null;
  const origin = container.getBoundingClientRect();
  const rectOf = (t: string) => {
    const el = caps.get(t);
    return el ? relRect(el, origin, container) : undefined;
  };

  const drawn = placeWires(wires, hoverKey, rectOf);
  if (drawn.length === 0) return null;

  return (
    <svg
      className="hkb-wires"
      width={container.scrollWidth}
      height={container.scrollHeight}
      aria-hidden="true"
    >
      <defs>
        {drawn.map((l) => (
          <mask key={l.id} id={`hkb-wire-mask-${l.id}`} maskUnits="userSpaceOnUse">
            <path className="hkb-wire-reveal" d={l.d} pathLength={1} />
          </mask>
        ))}
      </defs>
      {drawn.map((l) => {
        const { wire } = l;
        const cls = [
          'hkb-wire',
          l.dir === 'out' ? 'is-out' : 'is-in',
          wire.kind === 'stop' && 'is-stop',
          !wire.enabled && 'is-off',
        ]
          .filter(Boolean)
          .join(' ');
        const [ex, ey] = l.end;
        return (
          <g key={l.id} className={cls}>
            {/* 光晕用下层同几何的宽描边而不是 drop-shadow：filter 在遮罩揭开动画里会整段闪 */}
            <path className="hkb-wire-glow" d={l.d} mask={`url(#hkb-wire-mask-${l.id})`} />
            <path className="hkb-wire-line" d={l.d} mask={`url(#hkb-wire-mask-${l.id})`} />
            <circle className="hkb-wire-src" cx={l.start[0]} cy={l.start[1]} r={2} />
            {wire.kind === 'stop' ? (
              <rect
                className="hkb-wire-pad is-stop"
                x={ex - 2.5}
                y={ey - 2.5}
                width={5}
                height={5}
              />
            ) : (
              <circle
                className={`hkb-wire-pad ${wire.mode === 'hold' ? 'is-hold' : 'is-toggle'}`}
                cx={ex}
                cy={ey}
                r={3}
              />
            )}
          </g>
        );
      })}
    </svg>
  );
}
