export interface TargetRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export function targetBounds(rects: readonly TargetRect[]): TargetRect | undefined {
  const visible = rects.filter((rect) => rect.width > 0 && rect.height > 0);
  if (visible.length === 0) return undefined;
  const left = Math.min(...visible.map((rect) => rect.left));
  const top = Math.min(...visible.map((rect) => rect.top));
  const right = Math.max(...visible.map((rect) => rect.left + rect.width));
  const bottom = Math.max(...visible.map((rect) => rect.top + rect.height));
  return { left, top, width: right - left, height: bottom - top };
}
