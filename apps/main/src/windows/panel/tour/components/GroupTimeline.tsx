import { useEffect, useState } from 'react';
import './GroupTimeline.css';

type Cell = 'run' | 'pause' | 'idle';

interface Props {
  /** 示例组两条切换与一条长按的启动键名。 */
  keys: [string, string, string];
}

const BEAT_MS = 1100;

function prefersReducedMotion(): boolean {
  return (
    typeof window !== 'undefined' && window.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

/**
 * 互斥组四拍时间线：按 A → 按 B → 按住长按 → 松开。气泡里先给画面，再让用户按真键对照。
 * 减少动态效果时不跑定时器，直接展示四拍全貌。
 */
export default function GroupTimeline({ keys: [a, b, hold] }: Props) {
  const reduced = prefersReducedMotion();
  const [beat, setBeat] = useState(reduced ? 3 : 0);

  useEffect(() => {
    if (reduced) return;
    const timer = setInterval(() => setBeat((n) => (n + 1) % 4), BEAT_MS);
    return () => clearInterval(timer);
  }, [reduced]);

  const beats = [`按 ${a}`, `按 ${b}`, `按住 ${hold}`, `松开 ${hold}`];
  const lanes: { name: string; mode: 'toggle' | 'hold'; cells: Cell[] }[] = [
    { name: '宏A', mode: 'toggle', cells: ['run', 'idle', 'idle', 'idle'] },
    { name: '宏B', mode: 'toggle', cells: ['idle', 'run', 'pause', 'run'] },
    { name: '长按', mode: 'hold', cells: ['idle', 'idle', 'run', 'idle'] },
  ];

  return (
    <div className="group-timeline" aria-label="互斥组时间线示意">
      <span className="group-timeline__lane-name">按键</span>
      {beats.map((label, i) => (
        <span
          key={label}
          className={`group-timeline__beat${i === beat ? ' is-current' : ''}${i > beat ? ' is-future' : ''}`}
        >
          {label}
        </span>
      ))}
      {lanes.map((lane) => (
        <LaneRow key={lane.name} {...lane} beat={beat} />
      ))}
    </div>
  );
}

function LaneRow({
  name,
  mode,
  cells,
  beat,
}: {
  name: string;
  mode: 'toggle' | 'hold';
  cells: Cell[];
  beat: number;
}) {
  return (
    <>
      <span className="group-timeline__lane-name">{name}</span>
      {cells.map((cell, i) => (
        <span
          key={i}
          className={`group-timeline__cell is-${cell} is-${mode}${i > beat ? ' is-future' : ''}`}
        >
          {cell === 'pause' ? '⏸' : ''}
        </span>
      ))}
    </>
  );
}
