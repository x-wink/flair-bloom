import { useLayoutEffect, useRef, useState } from 'react';
import { runningFrameGeometry, type RunningFrameVariant } from '../runningFrame';
import FrameFlowCanvas from './FrameFlowCanvas';
import '../active-feedback.css';

export default function RunningFrame({
  flow,
  variant,
}: {
  flow: boolean;
  variant: RunningFrameVariant;
}) {
  const frameRef = useRef<HTMLDivElement>(null);
  const [geometry, setGeometry] = useState<ReturnType<typeof runningFrameGeometry>>();

  useLayoutEffect(() => {
    const frame = frameRef.current;
    if (!flow || !frame) return;
    const measure = () => {
      const style = getComputedStyle(frame);
      const next = runningFrameGeometry(
        frame.clientWidth,
        frame.clientHeight,
        Number.parseFloat(style.borderTopLeftRadius),
        variant,
      );
      setGeometry((current) =>
        current?.width === next?.width &&
        current?.height === next?.height &&
        current?.radius === next?.radius &&
        current?.variant === next?.variant
          ? current
          : next,
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(frame);
    return () => observer.disconnect();
  }, [flow, variant]);

  return (
    <div ref={frameRef} className="global-active-frame" aria-hidden="true">
      {flow && geometry && (
        <FrameFlowCanvas
          key={`${geometry.variant}:${geometry.width}:${geometry.height}:${geometry.radius}`}
          geometry={geometry}
        />
      )}
    </div>
  );
}
