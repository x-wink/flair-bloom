import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import Button from '../components/Button';
import { CloseIcon } from '../components/icons';
import { GAP, PAD, computePos, type Location } from '../components/Overlay';
import type { TourDef, TourExitResult, TourHost, TourSnapshot } from './types';
import './TourRunner.css';

interface Props {
  tour: TourDef;
  host: TourHost;
  /** 协议 / 更新弹窗出现时为 true：整层不渲染，步骤索引保留。 */
  paused?: boolean;
  onExit: (result: TourExitResult) => void;
}

interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** 高亮框比目标外扩的像素。 */
const HL_PAD = 4;
/** 找目标的最长等待，prepare 触发的重渲染通常一两帧就好。 */
const TARGET_WAIT_MS = 1500;
/** 实操完成后打勾停留的时间。 */
const DONE_LINGER_MS = 350;
/** 箭头离气泡边缘的最小距离。 */
const ARROW_MARGIN = 16;

const MAIN_SIDES: ReadonlySet<Location> = new Set(['top', 'bottom', 'left', 'right']);

function queryTarget(name: string): HTMLElement | null {
  // 规则卡内的锚点（rule-key / rule-enable / rule-advanced）每张卡都有一份，而实操判定看的是
  // 最后一条规则：老用户带着一列规则跑教程时，取全局首个匹配会高亮第一张卡、判定最后一张。
  // 先在 rule-latest 卡内找，找不到再退回全局。
  const sel = `[data-tour="${name}"]`;
  return (
    document.querySelector<HTMLElement>(`[data-tour="rule-latest"] ${sel}`) ??
    document.querySelector<HTMLElement>(sel)
  );
}

function waitForTarget(name: string, isCancelled: () => boolean): Promise<HTMLElement | null> {
  return new Promise((resolve) => {
    const deadline = performance.now() + TARGET_WAIT_MS;
    const tick = () => {
      if (isCancelled()) return resolve(null);
      const el = queryTarget(name);
      if (el) return resolve(el);
      if (performance.now() > deadline) return resolve(null);
      requestAnimationFrame(tick);
    };
    tick();
  });
}

function pickPlacement(rect: Rect, size: { w: number; h: number }): Location {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const below = vh - (rect.top + rect.height);
  const above = rect.top;
  const needV = size.h + GAP + PAD;
  if (below >= needV) return 'bottom';
  if (above >= needV) return 'top';
  const right = vw - (rect.left + rect.width);
  const left = rect.left;
  const needH = size.w + GAP + PAD;
  if (right >= needH) return 'right';
  if (left >= needH) return 'left';
  return below >= above ? 'bottom' : 'top';
}

function expand(r: Rect, pad: number): Rect {
  return {
    left: r.left - pad,
    top: r.top - pad,
    width: r.width + pad * 2,
    height: r.height + pad * 2,
  };
}

function sameRect(a: Rect | null, b: Rect | null): boolean {
  if (!a || !b) return a === b;
  return (
    Math.round(a.left) === Math.round(b.left) &&
    Math.round(a.top) === Math.round(b.top) &&
    Math.round(a.width) === Math.round(b.width) &&
    Math.round(a.height) === Math.round(b.height)
  );
}

interface BubbleLayout {
  left: number;
  top: number;
  placement: Location;
  /** 箭头相对气泡的偏移；非四个主方位时无箭头。 */
  arrow?: { left?: number; top?: number };
}

export default function TourRunner({ tour, host, paused = false, onExit }: Props) {
  const [index, setIndex] = useState(0);
  // preparing：跑 prepare 并找目标；shown：正常展示；done：实操已完成，打勾停留中
  const [phase, setPhase] = useState<'preparing' | 'shown' | 'done'>('preparing');
  const [targetEl, setTargetEl] = useState<HTMLElement | null>(null);
  const [missing, setMissing] = useState(false);
  const [rect, setRect] = useState<Rect | null>(null);
  const [layout, setLayout] = useState<BubbleLayout | null>(null);
  const [viewportTick, setViewportTick] = useState(0);

  const bubbleRef = useRef<HTMLDivElement>(null);
  const hostRef = useRef(host);
  hostRef.current = host;
  const onExitRef = useRef(onExit);
  onExitRef.current = onExit;
  const enteredRef = useRef<TourSnapshot>(host.snapshot);
  const directionRef = useRef<1 | -1>(1);
  const targetElRef = useRef<HTMLElement | null>(null);
  targetElRef.current = targetEl;

  const step = tour.steps[index];
  const total = tour.steps.length;
  const interactive = !!step.done;
  const isLast = index === total - 1;

  // 用 ref 读当前索引而不是在 setIndex 的更新函数里做副作用：StrictMode 会把更新函数
  // 跑两遍，onExit 就会被叫两次。
  const indexRef = useRef(index);
  indexRef.current = index;
  const go = useCallback(
    (delta: 1 | -1) => {
      directionRef.current = delta;
      const next = indexRef.current + delta;
      if (next >= total) {
        onExitRef.current('completed');
        return;
      }
      if (next >= 0) setIndex(next);
    },
    [total],
  );

  // 进入步骤：先 prepare（切页签、开设置），再等目标出现
  useEffect(() => {
    let cancelled = false;
    setPhase('preparing');
    setTargetEl(null);
    setRect(null);
    setLayout(null);
    setMissing(false);
    (async () => {
      const result = await step.prepare?.(hostRef.current);
      if (cancelled) return;
      if (result === 'skip') {
        // 第 0 步往回退没有去处：go(-1) 既不换步也不退出，整层会停在 preparing 只剩 Esc 能救。
        // 这种情况改向前找下一个能展示的步骤。
        const back = indexRef.current === 0 && directionRef.current === -1;
        go(back ? 1 : directionRef.current);
        return;
      }
      enteredRef.current = hostRef.current.snapshot;
      if (!step.target) {
        setPhase('shown');
        return;
      }
      const el = await waitForTarget(step.target, () => cancelled);
      if (cancelled) return;
      if (el) {
        el.scrollIntoView({ block: 'nearest' });
        setTargetEl(el);
      } else {
        setMissing(true);
      }
      setPhase('shown');
    })();
    return () => {
      cancelled = true;
    };
  }, [step, go]);

  // 跟随目标：每帧比对一次 rect，变了才 setState；目标脱离 DOM（页签重渲染）时重新查
  useEffect(() => {
    if (phase === 'preparing' || !step.target) return;
    const name = step.target;
    let raf = 0;
    let last: Rect | null = null;
    const tick = () => {
      let el = targetElRef.current;
      if (el && !el.isConnected) {
        el = queryTarget(name);
        setTargetEl(el);
      }
      if (el) {
        const r = el.getBoundingClientRect();
        const next = { left: r.left, top: r.top, width: r.width, height: r.height };
        if (!sameRect(last, next)) {
          last = next;
          setRect(next);
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [phase, step.target]);

  useEffect(() => {
    const onResize = () => setViewportTick((t) => t + 1);
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);

  // 气泡定位：量自身尺寸后按目标 rect 摆放；无目标时居中
  useLayoutEffect(() => {
    if (phase === 'preparing' || paused) return;
    const el = bubbleRef.current;
    if (!el) return;
    const size = { w: el.offsetWidth, h: el.offsetHeight };
    if (!rect) {
      setLayout({
        left: (window.innerWidth - size.w) / 2,
        top: (window.innerHeight - size.h) / 2,
        placement: 'bottom',
      });
      return;
    }
    const hl = expand(rect, HL_PAD);
    const placement = step.placement ?? pickPlacement(hl, size);
    const pos = computePos(hl, placement, [0, 0], size);
    if (!pos) return;
    const next: BubbleLayout = { left: pos.left, top: pos.top, placement };
    if (MAIN_SIDES.has(placement)) {
      const cx = hl.left + hl.width / 2 - pos.left;
      const cy = hl.top + hl.height / 2 - pos.top;
      next.arrow =
        placement === 'top' || placement === 'bottom'
          ? { left: Math.max(ARROW_MARGIN, Math.min(cx, size.w - ARROW_MARGIN)) }
          : { top: Math.max(ARROW_MARGIN, Math.min(cy, size.h - ARROW_MARGIN)) };
    }
    setLayout(next);
  }, [phase, paused, rect, step, missing, viewportTick]);

  // 实操判定：宿主快照每次变化都看一眼
  useEffect(() => {
    if (phase !== 'shown' || !step.done) return;
    if (step.done(host.snapshot, enteredRef.current)) setPhase('done');
  }, [host.snapshot, phase, step]);

  // 打勾停一下再前进。计时器单独一个 effect：放在上面那个里会被每次快照变化的
  // cleanup 清掉，永远走不到下一步。
  useEffect(() => {
    if (phase !== 'done') return;
    const timer = setTimeout(() => go(1), DONE_LINGER_MS);
    return () => clearTimeout(timer);
  }, [phase, go]);

  // Esc 退出整组。挂 capture 阶段并截断传播，OverlayRoot 的根级 Escape（冒泡阶段）才不会
  // 顺手关掉教程正在讲的设置弹窗；按键框录入中除外，那时 Esc 是用户在录一个键。
  // 教程被挡住（确认框、协议、更新公告）时不挂：那会儿气泡根本不可见，截断传播会让用户
  // 按 Esc 关不掉眼前的对话框，还把背后的教程悄悄跳掉。
  useEffect(() => {
    if (paused || phase === 'preparing') return;
    function onKey(e: KeyboardEvent) {
      if (e.key !== 'Escape') return;
      const active = document.activeElement;
      if (active instanceof HTMLElement && active.classList.contains('capturing')) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      onExitRef.current('skipped');
    }
    window.addEventListener('keydown', onKey, { capture: true });
    return () => window.removeEventListener('keydown', onKey, { capture: true });
  }, [paused, phase]);

  if (paused || phase === 'preparing') return null;

  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const hl = rect ? expand(rect, HL_PAD) : null;
  const hlRight = hl ? hl.left + hl.width : 0;
  const hlBottom = hl ? hl.top + hl.height : 0;
  const positioned = layout !== null;

  return createPortal(
    <>
      <div className="tour-mask" aria-hidden="true">
        {hl ? (
          <>
            <div
              className="tour-mask__part"
              style={{ left: 0, top: 0, width: vw, height: hl.top }}
            />
            <div
              className="tour-mask__part"
              style={{ left: 0, top: hlBottom, width: vw, height: Math.max(0, vh - hlBottom) }}
            />
            <div
              className="tour-mask__part"
              style={{ left: 0, top: hl.top, width: hl.left, height: hl.height }}
            />
            <div
              className="tour-mask__part"
              style={{
                left: hlRight,
                top: hl.top,
                width: Math.max(0, vw - hlRight),
                height: hl.height,
              }}
            />
            {/* 讲解步骤连目标也不让点，免得用户点了却没进入下一步、以为坏了 */}
            {!interactive && (
              <div
                className="tour-mask__blocker"
                style={{ left: hl.left, top: hl.top, width: hl.width, height: hl.height }}
              />
            )}
            <div
              className={`tour-highlight${phase === 'done' ? ' tour-highlight--done' : ''}`}
              style={{ left: hl.left, top: hl.top, width: hl.width, height: hl.height }}
            />
          </>
        ) : (
          <div className="tour-mask__part" style={{ left: 0, top: 0, width: vw, height: vh }} />
        )}
      </div>

      <div
        ref={bubbleRef}
        className={`tour-bubble tour-bubble--${layout?.placement ?? 'bottom'}${positioned ? '' : ' tour-bubble--measuring'}`}
        style={{ left: layout?.left ?? 0, top: layout?.top ?? 0 }}
        role="dialog"
        aria-label={`${tour.title}：${step.title}`}
      >
        {layout?.arrow && (
          <span className="tour-bubble__arrow" style={layout.arrow} aria-hidden="true" />
        )}
        <div className="tour-bubble__head">
          <span className="tour-bubble__title">{step.title}</span>
          <span className="tour-bubble__progress">
            {index + 1} / {total}
          </span>
          <button
            type="button"
            className="tour-bubble__close"
            onClick={() => onExit('skipped')}
            aria-label="退出教程"
            title="退出教程（Esc）"
          >
            <CloseIcon size={12} />
          </button>
        </div>
        <div className="tour-bubble__body">
          {step.body}
          {missing && (
            <p className="tour-bubble__note">这一步的目标当前不在界面上，可以直接看下一步。</p>
          )}
        </div>
        {interactive && (
          <div className={`tour-bubble__hint${phase === 'done' ? ' tour-bubble__hint--done' : ''}`}>
            {phase === 'done' ? '✓ 完成' : `● ${step.actionHint ?? '等你操作…'}`}
          </div>
        )}
        <div className="tour-bubble__footer">
          {interactive && phase !== 'done' && (
            <Button variant="text" tone="neutral" size="sm" onClick={() => go(1)}>
              跳过这步
            </Button>
          )}
          <span className="tour-bubble__spacer" />
          {index > 0 && (
            <Button variant="outline" tone="neutral" size="sm" onClick={() => go(-1)}>
              上一步
            </Button>
          )}
          {!interactive && (
            <Button size="sm" onClick={() => go(1)}>
              {isLast ? '完成' : '下一步'}
            </Button>
          )}
        </div>
      </div>
    </>,
    document.body,
  );
}
