import './IntervalInput.css';

interface Props {
  value: number;
  min: number;
  max: number;
  onChange: (v: number) => void;
  /** 步进按钮单次调节幅度（ms），默认 10 */
  step?: number;
}

/** 连发间隔输入：数值输入框 + 左右快速步进按钮，统一供竖版规则卡片与横版底栏使用。 */
export default function IntervalInput({ value, min, max, onChange, step = 10 }: Props) {
  const clamp = (v: number) => Math.max(min, Math.min(max, Math.round(v)));
  return (
    <div className="interval-input">
      <button
        type="button"
        className="interval-step"
        aria-label="减小间隔"
        disabled={value <= min}
        onClick={() => onChange(clamp(value - step))}
      >
        −
      </button>
      <input
        type="number"
        min={min}
        max={max}
        value={value}
        onChange={(e) => {
          const v = Number(e.target.value);
          if (Number.isNaN(v)) return;
          onChange(clamp(v));
        }}
      />
      <span className="interval-unit">ms</span>
      <button
        type="button"
        className="interval-step"
        aria-label="增大间隔"
        disabled={value >= max}
        onClick={() => onChange(clamp(value + step))}
      >
        +
      </button>
    </div>
  );
}
