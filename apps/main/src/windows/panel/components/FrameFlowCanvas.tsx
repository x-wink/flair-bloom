import { useLayoutEffect, useRef } from 'react';
import { FRAME_FLOW_COUNT, flowRibbon, framePoint, type FrameGeometry } from '../runningFrame';
import { parseHex } from '../contrast';

export default function FrameFlowCanvas({ geometry }: { geometry: FrameGeometry }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useLayoutEffect(() => {
    const canvas = ref.current;
    const context = canvas?.getContext('2d', { alpha: true, desynchronized: true });
    if (!canvas || !context) return;
    const width = geometry.width + geometry.inset * 2;
    const height = geometry.height + geometry.inset * 2;
    let body = '#ae9dff';
    let headColor = '#e5e7eb';
    let glow = 0.4;
    const palette = () => {
      const style = getComputedStyle(canvas);
      body =
        style.getPropertyValue('--fb-frame-flow-body').trim() ||
        style.getPropertyValue('--fb-color-primary-text').trim() ||
        '#ae9dff';
      headColor = style.color;
      glow = Number.parseFloat(style.getPropertyValue('--fb-frame-glow-alpha')) || 0.4;
    };
    const resize = () => {
      const scale = Math.min(window.devicePixelRatio || 1, 2);
      canvas.width = Math.ceil(width * scale);
      canvas.height = Math.ceil(height * scale);
      context.setTransform(scale, 0, 0, scale, 0, 0);
    };
    const rgba = (color: string, alpha: number) => {
      const { r, g, b } = parseHex(color);
      return `rgba(${r},${g},${b},${alpha})`;
    };
    const fill = (distance: number) => {
      const points = flowRibbon(geometry, distance);
      const tail = points[0];
      const head = framePoint(geometry, distance + geometry.headWidth * 0.7);
      const gradient = context.createLinearGradient(tail.x, tail.y, head.x, head.y);
      gradient.addColorStop(0, rgba(body, 0));
      gradient.addColorStop(0.25, rgba(body, 0.08));
      gradient.addColorStop(0.6, rgba(body, 0.45));
      gradient.addColorStop(0.88, body);
      gradient.addColorStop(0.97, headColor);
      gradient.addColorStop(1, headColor);
      context.fillStyle = gradient;
      context.shadowColor = rgba(body, glow);
      context.shadowBlur = 2;
      context.beginPath();
      context.moveTo(points[0].x, points[0].y);
      for (let i = 1; i < points.length; i++) context.lineTo(points[i].x, points[i].y);
      context.closePath();
      context.fill();
    };
    let animation = 0;
    let start: number | undefined;
    const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
    const draw = (timestamp: number) => {
      start ??= timestamp;
      const phase = ((timestamp - start) / (geometry.duration * 1000)) % 1;
      context.clearRect(0, 0, width, height);
      for (let i = 0; i < FRAME_FLOW_COUNT; i++) {
        const distance = (phase + i / FRAME_FLOW_COUNT) * geometry.perimeter;
        fill(distance);
      }
      animation = requestAnimationFrame(draw);
    };
    const restart = () => {
      cancelAnimationFrame(animation);
      context.clearRect(0, 0, width, height);
      start = undefined;
      if (!motion.matches && !document.hidden) animation = requestAnimationFrame(draw);
    };
    palette();
    resize();
    restart();
    // 主题读取放在变更回调中，避免逐帧读取样式触发布局同步。
    const observer = new MutationObserver(palette);
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['style', 'data-theme'],
    });
    motion.addEventListener('change', restart);
    document.addEventListener('visibilitychange', restart);
    window.addEventListener('resize', resize);
    return () => {
      cancelAnimationFrame(animation);
      observer.disconnect();
      motion.removeEventListener('change', restart);
      document.removeEventListener('visibilitychange', restart);
      window.removeEventListener('resize', resize);
    };
  }, [geometry]);
  return <canvas ref={ref} className="global-frame-flow" />;
}
