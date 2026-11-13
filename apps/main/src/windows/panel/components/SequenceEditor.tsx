import { useState } from 'react';
import KeyCapture, { type CaptureReject, type SlotPolicy } from './KeyCapture';
import IntervalInput from './IntervalInput';
import { MAX_OUTPUT_STEPS, validateSteps, type OutputStep } from '../outputSequence';
import SequenceRecorder from './SequenceRecorder';
import { severityForKey, type Conflict } from '../conflicts';

interface Props {
  steps: OutputStep[];
  policy: SlotPolicy;
  min: number;
  max: number;
  onReject: (info: CaptureReject) => void;
  onChange: (steps: OutputStep[]) => void;
  conflicts?: readonly Conflict[];
}

export default function SequenceEditor({
  steps,
  policy,
  min,
  max,
  onReject,
  onChange,
  conflicts,
}: Props) {
  const [error, setError] = useState<string>();

  const change = (next: OutputStep[]) => {
    const message = validateSteps(next, min, max);
    setError(message);
    if (!message) onChange(next);
  };

  const move = (index: number, offset: number) => {
    const next = [...steps];
    [next[index], next[index + offset]] = [next[index + offset], next[index]];
    change(next);
  };

  return (
    <section
      className={`sequence-editor${steps.length === 1 && !error ? ' is-single-step' : ''}`}
      aria-label="连发序列"
      data-tour="sequence-editor"
      onDragStart={(event) => event.stopPropagation()}
    >
      <div className="sequence-heading">
        <span>连发序列</span>
        <span className="sequence-summary">
          {steps.length} 步 · {steps.reduce((sum, step) => sum + step.interval_ms, 0)}ms / 轮
        </span>
      </div>
      {steps.length > 1 && (
        <ol className="sequence-steps">
          {steps.map((step, index) => (
            <li key={index} className="sequence-step">
              <span className="sequence-index">{index + 1}</span>
              <KeyCapture
                policy={policy}
                value={step.key}
                conflict={conflicts ? severityForKey(conflicts, step.key) : null}
                onReject={onReject}
                onChange={(key) =>
                  key && change(steps.map((item, i) => (i === index ? { ...item, key } : item)))
                }
              />
              <IntervalInput
                value={step.interval_ms}
                min={min}
                max={max}
                onChange={(interval_ms) =>
                  change(steps.map((item, i) => (i === index ? { ...item, interval_ms } : item)))
                }
              />
              <div className="sequence-step-actions">
                <button
                  type="button"
                  disabled={index === 0}
                  aria-label={`上移第 ${index + 1} 步`}
                  title="上移"
                  onClick={() => move(index, -1)}
                >
                  ↑
                </button>
                <button
                  type="button"
                  disabled={index === steps.length - 1}
                  aria-label={`下移第 ${index + 1} 步`}
                  title="下移"
                  onClick={() => move(index, 1)}
                >
                  ↓
                </button>
                <button
                  type="button"
                  disabled={steps.length >= MAX_OUTPUT_STEPS}
                  aria-label={`复制第 ${index + 1} 步`}
                  title="复制"
                  onClick={() =>
                    change([...steps.slice(0, index + 1), { ...step }, ...steps.slice(index + 1)])
                  }
                >
                  ⧉
                </button>
                <button
                  type="button"
                  disabled={steps.length === 1}
                  aria-label={`删除第 ${index + 1} 步`}
                  title="删除"
                  onClick={() => change(steps.filter((_, i) => i !== index))}
                >
                  ×
                </button>
              </div>
            </li>
          ))}
        </ol>
      )}
      {error && <p role="alert">{error}</p>}
      <SequenceRecorder
        onRecorded={change}
        canAddStep={steps.length < MAX_OUTPUT_STEPS}
        onAddStep={() => change([...steps, { ...steps[steps.length - 1] }])}
      />
    </section>
  );
}
