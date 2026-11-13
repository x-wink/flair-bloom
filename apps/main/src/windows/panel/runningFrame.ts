export const FRAME_FLOW_COUNT = 2;
export const FRAME_LAP_DURATION = 1;
export type RunningFrameVariant = 'vertical' | 'horizontal' | 'float';

const FRAME_SIZES = {
  float: { maxTail: 120, headWidth: 8, headHeight: 5, lean: 1.5 },
  vertical: { maxTail: 320, headWidth: 12, headHeight: 8, lean: 2.5 },
  horizontal: { maxTail: 480, headWidth: 14, headHeight: 10, lean: 3 },
};

export function runningFrameGeometry(
  width: number,
  height: number,
  outerRadius: number,
  variant: RunningFrameVariant = 'vertical',
) {
  if (![width, height, outerRadius].every(Number.isFinite) || width <= 4 || height <= 4)
    return undefined;
  // 轨迹压住描边内缘半像素，避免抗锯齿留下细缝。
  const inset = variant === 'float' ? 0.5 : 1.5;
  const innerWidth = width - inset * 2;
  const innerHeight = height - inset * 2;
  const radius = Math.max(0, Math.min(outerRadius - inset, innerWidth / 2, innerHeight / 2));
  const perimeter = 2 * (innerWidth + innerHeight - radius * 4) + 2 * Math.PI * radius;
  const size = FRAME_SIZES[variant];
  return {
    width: innerWidth,
    height: innerHeight,
    radius,
    perimeter,
    inset,
    variant,
    headWidth: size.headWidth,
    headHeight: size.headHeight,
    lean: Math.min(size.lean, Math.max(0, Math.min(innerWidth, innerHeight) / 2 - 0.5)),
    beamLength: Math.min(size.maxTail, perimeter * 0.22),
    duration: FRAME_LAP_DURATION,
  };
}

export type FrameGeometry = NonNullable<ReturnType<typeof runningFrameGeometry>>;
export type FramePoint = { x: number; y: number };

export function framePoint(frame: FrameGeometry, distance: number) {
  let remaining = ((distance % frame.perimeter) + frame.perimeter) % frame.perimeter;
  const { inset, width, height, radius } = frame;
  const right = inset + width;
  const bottom = inset + height;
  const arc = (Math.PI * radius) / 2;
  for (let side = 0; side < 4; side++) {
    const length = (side % 2 === 0 ? width : height) - radius * 2;
    if (remaining < length) {
      switch (side) {
        case 0:
          return { x: inset + radius + remaining, y: inset, nx: 0, ny: 1 };
        case 1:
          return { x: right, y: inset + radius + remaining, nx: -1, ny: 0 };
        case 2:
          return { x: right - radius - remaining, y: bottom, nx: 0, ny: -1 };
        default:
          return { x: inset, y: bottom - radius - remaining, nx: 1, ny: 0 };
      }
    }
    remaining -= length;
    if (remaining < arc) {
      const angle = -Math.PI / 2 + (side * Math.PI) / 2 + remaining / radius;
      const cx = side < 2 ? right - radius : inset + radius;
      const cy = side === 0 || side === 3 ? inset + radius : bottom - radius;
      const cos = Math.cos(angle);
      const sin = Math.sin(angle);
      return { x: cx + radius * cos, y: cy + radius * sin, nx: -cos, ny: -sin };
    }
    remaining -= arc;
  }
  return { x: inset + radius, y: inset, nx: 0, ny: 1 };
}

export function flowRibbon(frame: FrameGeometry, headDistance: number) {
  const tail = frame.beamLength;
  const nose = frame.headWidth * 0.7;
  const samples = Math.max(32, Math.ceil((tail + nose) / 3));
  const outer: FramePoint[] = [];
  const inner: FramePoint[] = [];
  for (let i = 0; i <= samples; i++) {
    const distance = -tail + ((tail + nose) * i) / samples;
    const progress = Math.min(1, Math.max(0, 1 + distance / frame.beamLength));
    const point = framePoint(frame, headDistance + distance);
    const lean = frame.lean * progress ** 1.7 + Math.max(0, distance) * 0.12;
    const taper = distance > 0 ? Math.sqrt(Math.max(0, 1 - (distance / nose) ** 2)) : 1;
    const halfWidth = frame.headHeight * 0.35 * progress ** 2.6;
    outer.push({
      x: point.x,
      y: point.y,
    });
    inner.push({
      x: point.x + point.nx * (lean + halfWidth) * taper,
      y: point.y + point.ny * (lean + halfWidth) * taper,
    });
  }
  return [...outer, ...inner.reverse()];
}
